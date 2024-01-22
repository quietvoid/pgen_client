use futures::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    app::PGenAppUpdate,
    calibration::ReadingTarget,
    generators::{
        start_tcp_generator_client, GeneratorClient, GeneratorClientCmd, GeneratorInterface,
    },
    pgen::{
        controller::{PGenControllerCmd, PGenControllerHandle},
        pattern_config::PGenPatternConfig,
    },
    spotread::{start_spotread_worker, SpotreadCmd},
};

#[derive(Debug, Clone)]
pub enum ExternalJobCmd {
    StartGeneratorClient(GeneratorClient),
    StopGeneratorClient(GeneratorClient),

    // spotread
    StartSpotreadProcess,
    StopSpotreadProcess,
    SpotreadMeasure((PGenPatternConfig, ReadingTarget)),
    SpotreadDoneMeasuring,
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
                            ExternalJobCmd::StartSpotreadProcess => {
                                log::trace!("spotread: Starting process");
                                match start_spotread_worker(app_tx.clone(), tx.clone(), controller_handle.clone()) {
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
                            ExternalJobCmd::SpotreadMeasure(info) => {
                                if let Some(spotread_tx) = spotread_tx.as_ref() {
                                    spotread_tx.try_send(SpotreadCmd::DoReading(info)).ok();
                                }
                            }
                            ExternalJobCmd::SpotreadDoneMeasuring => {
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
