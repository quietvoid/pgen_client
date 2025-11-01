use std::{iter::once, process::Stdio, time::Duration};

use anyhow::{Result, anyhow, bail};
use futures::{FutureExt, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::mpsc::Sender,
};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    app::PGenAppUpdate,
    calibration::{CalibrationTarget, PatternInsertionConfig, ReadingResult},
    external::ExternalJobCmd,
    pgen::{controller::PGenControllerHandle, pattern_config::PGenPatternConfig},
    utils::pattern_cfg_set_colour_from_float_level,
};

const EXPECTED_INIT_LINE: &str = "Place instrument on spot to be measured";
const READING_READY_SUBSTR: &str = "take a reading:";
const READING_RESULT_SUBSTR: &str = "Result is XYZ";

#[derive(Debug)]
struct SpotreadProc {
    child: Child,
    err_lines: Lines<BufReader<ChildStderr>>,

    reader: BufReader<ChildStdout>,
    read_buf: Vec<u8>,
    can_take_reading: bool,
    writer: BufWriter<ChildStdin>,

    app_tx: Sender<PGenAppUpdate>,
}

#[derive(Debug)]
pub enum SpotreadCmd {
    DoReading(SpotreadReadingConfig),
    Exit,
}

#[derive(Debug, Clone, Copy)]
pub struct SpotreadReadingConfig {
    pub target: CalibrationTarget,
    pub pattern_cfg: PGenPatternConfig,
    pub pattern_insertion_cfg: PatternInsertionConfig,
}

pub fn start_spotread_worker(
    app_tx: Sender<PGenAppUpdate>,
    external_tx: Sender<ExternalJobCmd>,
    controller_handle: PGenControllerHandle,
    cli_args: Vec<(String, Option<String>)>,
) -> Result<Sender<SpotreadCmd>> {
    let (tx, rx) = tokio::sync::mpsc::channel(5);
    let mut rx = ReceiverStream::new(rx).fuse();

    let mut spotread_proc = tokio::task::block_in_place(|| {
        let mut spotread_proc = SpotreadProc::new(app_tx.clone(), cli_args)?;
        let mut init_line = String::with_capacity(64);

        tokio::runtime::Handle::current().block_on(async {
            loop {
                futures::select! {
                    err_line = spotread_proc.err_lines.next_line().fuse() => {
                        if let Ok(Some(line)) = err_line
                            && line.starts_with("Diagnostic") {
                                log::error!("Something failed: {line}");
                                spotread_proc.exit_logged(false).await;

                                app_tx.try_send(PGenAppUpdate::SpotreadStarted(false)).ok();
                                bail!("Failed starting spotread");
                            }
                    }
                    res = spotread_proc.reader.read_line(&mut init_line).fuse() => match res {
                        Ok(_) => {
                            if init_line.trim().contains(EXPECTED_INIT_LINE) {
                                log::trace!("init line: {init_line:?}");
                                spotread_proc.read_until_take_reading_ready().await?;

                                break;
                            }
                        },
                        Err(e) => {
                            log::error!("init: {e}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                }
            }

            Ok::<SpotreadProc, anyhow::Error>(spotread_proc)
        })
    })?;

    tokio::spawn(async move {
        loop {
            futures::select! {
                err_line = spotread_proc.err_lines.next_line().fuse() => {
                    if let Ok(Some(line)) = err_line
                        && line.starts_with("Diagnostic") {
                            log::error!("Something failed: {line}");
                            spotread_proc.exit_logged(false).await;

                            app_tx.try_send(PGenAppUpdate::SpotreadStarted(false)).ok();
                            break;
                        }
                }
                msg = rx.select_next_some() => {
                    match msg {
                        SpotreadCmd::DoReading(SpotreadReadingConfig { target, pattern_cfg, pattern_insertion_cfg }) => {
                            // ready process stdout before sending patch
                            // because the result must be sent asap and flushing stdout would delay result handling
                            spotread_proc.read_until_take_reading_ready().await.ok();

                            {
                                let mut controller = controller_handle.lock().await;

                                let wait_duration = if pattern_insertion_cfg.enabled {
                                    let mut inserted_pattern_cfg = pattern_cfg;
                                    pattern_cfg_set_colour_from_float_level(&mut inserted_pattern_cfg, pattern_insertion_cfg.level);

                                    controller.send_pattern_and_wait(inserted_pattern_cfg, pattern_insertion_cfg.duration).await;

                                    // Leave more time for the display to adjust after inserted pattern
                                    Duration::from_secs_f64(1.5)
                                } else {
                                    Duration::from_secs_f64(0.5)
                                };

                                controller.send_pattern_and_wait(pattern_cfg, wait_duration).await;
                            }

                            let res = tokio::time::timeout(Duration::from_secs(30), spotread_proc.try_measure(target)).await;

                            match res {
                                Ok(res) => {
                                    if let Err(e) = res {
                                        app_tx.try_send(PGenAppUpdate::SpotreadRes(None)).ok();
                                        log::error!("Failed taking measure {e}");
                                    }
                                }
                                Err(_) => {
                                    log::error!("Timed out trying to measure patch");
                                }
                            }

                            external_tx.try_send(ExternalJobCmd::SpotreadDoneMeasuring).ok();
                        },
                        SpotreadCmd::Exit => {
                            log::trace!("requested exit");
                            spotread_proc.exit_logged(true).await;

                            app_tx.try_send(PGenAppUpdate::SpotreadStarted(false)).ok();
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(tx)
}

impl SpotreadProc {
    pub fn new(
        app_tx: Sender<PGenAppUpdate>,
        cli_args: Vec<(String, Option<String>)>,
    ) -> Result<Self> {
        let args_iter = cli_args
            .into_iter()
            .flat_map(|kv| once(kv.0).chain(once(kv.1.unwrap_or_default())))
            .filter(|a| !a.is_empty());

        let mut child = Command::new("spotread")
            .args(args_iter)
            .env("ARGYLL_NOT_INTERACTIVE", "1")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let child_err = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("child did not have a handle to stderr"))?;
        let err_reader = BufReader::new(child_err);
        let err_lines = err_reader.lines();

        let child_out = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("child did not have a handle to stdout"))?;
        let reader = BufReader::new(child_out);

        let child_in = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("child did not have a handle to stdin"))?;
        let writer = BufWriter::new(child_in);

        Ok(Self {
            child,
            err_lines,
            reader,
            read_buf: Vec::with_capacity(1024),
            writer,
            can_take_reading: false,
            app_tx,
        })
    }

    async fn try_measure(&mut self, target: CalibrationTarget) -> Result<()> {
        self.read_until_take_reading_ready().await?;

        // Take reading by sending enter
        self.writer.write_all("\n".as_bytes()).await?;
        self.writer.flush().await?;

        let mut line = String::with_capacity(32);

        loop {
            line.clear();

            self.reader.read_line(&mut line).await?;

            log::trace!("Raw output line: {line:?}");
            let final_line = line.trim();

            if final_line.is_empty() {
                continue;
            }

            if final_line.starts_with(READING_RESULT_SUBSTR) {
                let reading = ReadingResult::from_spotread_result(target, final_line)?;
                log::info!("{reading:?}");

                self.app_tx
                    .send(PGenAppUpdate::SpotreadRes(Some(reading)))
                    .await
                    .ok();
                break;
            } else if final_line.starts_with("Spot read failed") {
                bail!(final_line.to_string());
            }
        }

        self.can_take_reading = false;

        Ok(())
    }

    pub async fn read_until_take_reading_ready(&mut self) -> Result<()> {
        if !self.can_take_reading {
            self.read_buf.clear();

            loop {
                let buf = self.reader.fill_buf().await?;
                let len = buf.len();

                self.read_buf.extend_from_slice(buf);
                self.reader.consume(len);

                let stdout = str::from_utf8(&self.read_buf)?;

                log::trace!("read_until_take_reading_ready[{len}] {stdout:?}");

                if stdout.trim().ends_with(READING_READY_SUBSTR) {
                    self.can_take_reading = true;

                    log::debug!("ready to take reading");

                    break;
                }
            }
        }

        Ok(())
    }

    async fn exit_logged(self, interactive: bool) {
        if let Err(e) = self.exit(interactive).await {
            log::error!("Failed exiting program: {e}");
        } else {
            log::trace!("process successfully exited");
        }
    }

    async fn exit(mut self, interactive: bool) -> Result<()> {
        if interactive {
            log::trace!("graceful interactive exit");

            self.read_until_take_reading_ready().await?;

            let mut out = String::with_capacity(32);

            self.writer.write_all("q\r\n".as_bytes()).await?;
            self.writer.flush().await?;

            loop {
                self.reader.read_line(&mut out).await?;

                if !out.trim().is_empty() {
                    break;
                }
            }

            self.writer.write_all("q\r\n".as_bytes()).await?;
            self.writer.flush().await?;

            log::trace!("exit output: {out:?}");
        }

        log::trace!("waiting for process to exit");
        let status = self.child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            bail!("process exited with status {status}");
        }
    }
}
