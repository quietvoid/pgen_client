use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use crate::{app::PGenAppUpdate, pgen::controller::PGenControllerCmd};

pub mod resolve;
pub mod tcp_generator_client;

pub use tcp_generator_client::{
    start_tcp_generator_client, TcpGeneratorClient, TcpGeneratorInterface,
};

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct GeneratorState {
    pub interface: GeneratorInterface,
    pub listening: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub enum GeneratorInterface {
    Tcp(TcpGeneratorInterface),
}

#[derive(Debug, Clone)]
pub enum GeneratorCmd {
    StartInterface(GeneratorInterface),
    StopInterface(GeneratorInterface),
}

#[derive(Debug, Clone)]
pub enum GeneratorClientCmd {
    Shutdown,
}

pub fn start_generator_worker(
    app_tx: Sender<PGenAppUpdate>,
    controller_tx: Sender<PGenControllerCmd>,
) -> Sender<GeneratorCmd> {
    let mut client_tx = None;

    let (tx, rx) = tokio::sync::mpsc::channel(5);
    let mut rx = ReceiverStream::new(rx).fuse();

    {
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                futures::select! {
                    cmd = rx.select_next_some() => {
                        match cmd {
                            GeneratorCmd::StartInterface(interface) => {
                                app_tx.try_send(PGenAppUpdate::Processing).ok();
                                log::trace!("Generator: Starting interface {interface:?}");

                                match interface {
                                    GeneratorInterface::Tcp(tcp_interface) => {
                                        if let Ok(tx) = start_tcp_generator_client(controller_tx.clone(), tx.clone(), tcp_interface).await {
                                            client_tx.replace(tx);
                                        }
                                    }
                                };

                                if client_tx.is_some() {
                                    app_tx.try_send(PGenAppUpdate::GeneratorListening(true)).ok();
                                }
                                app_tx.try_send(PGenAppUpdate::DoneProcessing).ok();
                            },
                            GeneratorCmd::StopInterface(interface) => {
                                log::trace!("Generator: Stopping interface {interface:?}");

                                if let Some(client_tx) = client_tx.take() {
                                    client_tx.send(GeneratorClientCmd::Shutdown).await.ok();
                                }

                                app_tx.try_send(PGenAppUpdate::GeneratorListening(false)).ok();
                            },
                        }
                    }
                }
            }
        });
    }

    tx
}
