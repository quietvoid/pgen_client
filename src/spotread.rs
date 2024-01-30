use std::{iter::once, process::Stdio, time::Duration};

use anyhow::{anyhow, bail, Result};
use futures::{FutureExt, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, Lines},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::mpsc::Sender,
};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    app::PGenAppUpdate,
    calibration::{ReadingResult, ReadingTarget},
    external::ExternalJobCmd,
    pgen::{controller::PGenControllerHandle, pattern_config::PGenPatternConfig},
};

struct SpotreadProc {
    child: Child,
    err_lines: Lines<BufReader<ChildStderr>>,

    reader: ChildStdout,
    read_buf: Vec<u8>,
    can_take_reading: bool,
    writer: BufWriter<ChildStdin>,

    app_tx: Sender<PGenAppUpdate>,
}

pub enum SpotreadCmd {
    DoReading((PGenPatternConfig, ReadingTarget)),
    Exit,
}

pub fn start_spotread_worker(
    app_tx: Sender<PGenAppUpdate>,
    external_tx: Sender<ExternalJobCmd>,
    controller_handle: PGenControllerHandle,
    cli_args: Vec<(String, String)>,
) -> Result<Sender<SpotreadCmd>> {
    let (tx, rx) = tokio::sync::mpsc::channel(5);
    let mut rx = ReceiverStream::new(rx).fuse();

    let mut spotread_proc = tokio::task::block_in_place(|| {
        let mut spotread_proc = SpotreadProc::new(app_tx.clone(), cli_args)?;

        tokio::runtime::Handle::current().block_on(async {
            loop {
                futures::select! {
                    err_line = spotread_proc.err_lines.next_line().fuse() => {
                        if let Ok(Some(line)) = err_line {
                            if line.starts_with("Diagnostic") {
                                log::error!("spotread: Something failed: {line}");
                                spotread_proc.exit_logged(false).await;

                                app_tx.try_send(PGenAppUpdate::SpotreadStarted(false)).ok();
                                bail!("Failed starting spotread");
                            }
                        }
                    }
                    bytes = spotread_proc.reader.read_u8().fuse() => match bytes {
                        Ok(_) => break,
                        Err(e) => {
                            log::trace!("Failed reading: {e}");
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
                    if let Ok(Some(line)) = err_line {
                        if line.starts_with("Diagnostic") {
                            log::error!("spotread: Something failed: {line}");
                            spotread_proc.exit_logged(false).await;

                            app_tx.try_send(PGenAppUpdate::SpotreadStarted(false)).ok();
                            break;
                        }
                    }
                }
                msg = rx.select_next_some() => {
                    match msg {
                        SpotreadCmd::DoReading((config, target)) => {
                            {
                                let mut controller = controller_handle.lock().await;
                                controller.send_pattern_and_wait(config).await;
                            }

                            let res = tokio::time::timeout(Duration::from_secs(20), spotread_proc.try_measure(target)).await;

                            let mut success = false;
                            match res {
                                Ok(res) => {
                                    if let Err(e) = res {
                                        log::error!("spotread: Failed taking measure {e}");
                                    } else {
                                        success = true;
                                    }
                                }
                                Err(_) => {
                                    log::error!("Timed out trying to measure patch");
                                }
                            }

                            if !success {
                                app_tx.try_send(PGenAppUpdate::SpotreadRes(None)).ok();
                            }
                            external_tx.try_send(ExternalJobCmd::SpotreadDoneMeasuring).ok();
                        },
                        SpotreadCmd::Exit => {
                            log::trace!("spotread: requested exit");
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
    pub fn new(app_tx: Sender<PGenAppUpdate>, cli_args: Vec<(String, String)>) -> Result<Self> {
        let mut child = Command::new("spotread")
            .args(
                cli_args
                    .iter()
                    .flat_map(|kv| once(kv.0.as_str()).chain(once(kv.1.as_str()))),
            )
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

        let child_in = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("child did not have a handle to stdin"))?;
        let writer = BufWriter::new(child_in);

        Ok(Self {
            child,
            err_lines,
            reader: child_out,
            read_buf: vec![0; 512],
            writer,
            can_take_reading: false,
            app_tx,
        })
    }

    async fn try_measure(&mut self, target: ReadingTarget) -> Result<()> {
        if self.can_take_reading {
            self.can_take_reading = false;

            // Take reading by sending enter
            self.writer.write_all("\n".as_bytes()).await?;
            self.writer.flush().await?;
        } else {
            // Flush stdout until we can read
            if let Some(lines) = self.read_stdout_lines().await? {
                if lines.iter().any(|e| e.contains("take a reading")) {
                    // Take reading by sending enter
                    self.writer.write_all("\n".as_bytes()).await?;
                    self.writer.flush().await?;
                }
            }
        }

        let mut err = None;

        // Read result and rest of stdouf buffer
        if let Some(lines) = self.read_stdout_lines().await? {
            for line in lines {
                if line.starts_with("Result is XYZ:") {
                    let reading = ReadingResult::new(target, &line)?;
                    self.app_tx
                        .send(PGenAppUpdate::SpotreadRes(Some(reading)))
                        .await
                        .ok();
                } else if line.starts_with("Spot read failed") {
                    err.replace(line.to_owned());
                } else if line.contains("take a reading") {
                    // Next reading won't need to read stdout
                    self.can_take_reading = true;
                }
            }
        }

        if let Some(err) = err {
            bail!(err);
        } else {
            Ok(())
        }
    }

    async fn read_stdout_lines(&mut self) -> Result<Option<Vec<String>>> {
        let num_bytes = self.reader.read(&mut self.read_buf).await?;
        if num_bytes == 0 {
            return Ok(None);
        }

        let output = std::str::from_utf8(&self.read_buf[..num_bytes])?;
        let lines: Vec<String> = output
            .lines()
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(|e| e.to_owned())
            .collect();

        Ok(Some(lines))
    }

    async fn exit_logged(self, interactive: bool) {
        if let Err(e) = self.exit(interactive).await {
            log::error!("spotread: Failed exiting program: {e}");
        } else {
            log::trace!("spotread: process successfully exited");
        }
    }

    async fn exit(mut self, interactive: bool) -> Result<()> {
        if interactive {
            log::trace!("spotread: graceful interactive exit");
            self.writer.write_all("q\r\n".as_bytes()).await?;
            self.writer.flush().await?;

            tokio::time::sleep(Duration::from_millis(500)).await;
            self.writer.write_all("q\r\n".as_bytes()).await?;
            self.writer.flush().await?;

            // Flush stdout to logs
            self.read_stdout_lines().await?;
        }

        log::trace!("spotread: waiting for process to exit");
        let status = self.child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            bail!("process exited with status {status}");
        }
    }
}
