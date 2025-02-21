use futures::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    app::{PGenAppUpdate, ReadFileType},
    generators::{
        GeneratorClient, GeneratorClientCmd, GeneratorInterface, start_tcp_generator_client,
    },
    pgen::controller::{PGenControllerCmd, PGenControllerHandle},
    spotread::{SpotreadCmd, SpotreadReadingConfig, start_spotread_worker},
};

#[derive(Debug, Clone)]
pub enum ExternalJobCmd {
    StartGeneratorClient(GeneratorClient),
    StopGeneratorClient(GeneratorClient),

    // spotread
    StartSpotreadProcess(Vec<(String, Option<String>)>),
    StopSpotreadProcess,
    SpotreadMeasure(SpotreadReadingConfig),
    SpotreadDoneMeasuring,

    ReadFile(ReadFileType),
}

pub fn start_external_jobs_worker(
    app_tx: Sender<PGenAppUpdate>,
    controller_tx: Sender<PGenControllerCmd>,
    controller_handle: PGenControllerHandle,
) -> Sender<ExternalJobCmd> {
    let mut gen_client_tx = None;
    let mut spotread_tx = None;

    let (tx, rx) = tokio::sync::mpsc::channel(5);
    let mut rx = ReceiverStream::new(rx).fuse();

    {
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                futures::select! {
                    cmd = rx.select_next_some() => {
                        app_tx.try_send(PGenAppUpdate::Processing).ok();

                        match cmd {
                            ExternalJobCmd::StartGeneratorClient(client) => {
                                log::trace!("Generator: Starting client {client:?}");

                                match client.interface() {
                                    GeneratorInterface::Tcp(tcp_interface) => {
                                        let client_fut = start_tcp_generator_client(app_tx.clone(), controller_tx.clone(), tx.clone(), tcp_interface);
                                        if let Ok(tx) = client_fut.await {
                                            gen_client_tx.replace(tx);
                                        }
                                    }
                                };

                                if gen_client_tx.is_some() {
                                    app_tx.try_send(PGenAppUpdate::GeneratorListening(true)).ok();
                                }
                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            },
                            ExternalJobCmd::StopGeneratorClient(client) => {
                                if let Some(client_tx) = gen_client_tx.take() {
                                    log::trace!("Generator: Stopping client {client:?}");
                                    client_tx.try_send(GeneratorClientCmd::Shutdown).ok();
                                } else {
                                    app_tx.try_send(PGenAppUpdate::GeneratorListening(false)).ok();
                                }
                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            },
                            ExternalJobCmd::StartSpotreadProcess(cli_args) => {
                                log::trace!("spotread: Starting process");
                                match start_spotread_worker(app_tx.clone(), tx.clone(), controller_handle.clone(), cli_args) {
                                    Ok(tx) => {
                                        spotread_tx.replace(tx);
                                        app_tx.try_send(PGenAppUpdate::SpotreadStarted(true)).ok();
                                    }
                                    Err(e) => {
                                        log::error!("spotread: Start failed: {e}");
                                    }
                                }
                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            }
                            ExternalJobCmd::StopSpotreadProcess => {
                                if let Some(tx) = spotread_tx.take() {
                                    log::trace!("spotread: Stopping process");
                                    tx.try_send(SpotreadCmd::Exit).ok();
                                } else {
                                    app_tx.try_send(PGenAppUpdate::SpotreadStarted(false)).ok();
                                }
                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            }
                            ExternalJobCmd::SpotreadMeasure(config) => {
                                if let Some(spotread_tx) = spotread_tx.as_ref() {
                                    spotread_tx.try_send(SpotreadCmd::DoReading(config)).ok();
                                }
                            }
                            ExternalJobCmd::SpotreadDoneMeasuring => {
                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            }
                            ExternalJobCmd::ReadFile(file_type) => {
                                let title = file_type.title();

                                let mut dialog = rfd::FileDialog::new().set_title(title);
                                for (filter_name, exts) in file_type.filters() {
                                    dialog = dialog.add_filter(*filter_name, exts);
                                }

                                if let Some(path) = dialog.pick_file() {
                                    app_tx.try_send(PGenAppUpdate::ReadFileResponse(file_type, path)).ok();
                                }

                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            }
                        }
                    }
                }
            }
        });
    }

    tx
}
