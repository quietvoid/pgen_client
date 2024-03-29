use std::{io::ErrorKind, sync::Arc};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tokio::{io::AsyncWriteExt, net::TcpStream, sync::RwLock};
use tokio_stream::wrappers::ReceiverStream;

use crate::app::PGenAppUpdate;
use crate::external::ExternalJobCmd;
use crate::pgen::controller::PGenControllerCmd;

use super::resolve::{
    handle_resolve_pattern_message, handle_resolve_tcp_stream_message, resolve_connect_tcp_stream,
};
use super::{GeneratorClientCmd, GeneratorInterface};

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub enum TcpGeneratorInterface {
    #[default]
    Resolve,
}

pub struct TcpGeneratorClient {
    pub stream: TcpStream,
    pub interface: TcpGeneratorInterface,
    pub buf: Vec<u8>,
    running: bool,

    controller_tx: Sender<PGenControllerCmd>,
    external_tx: Sender<ExternalJobCmd>,
}
pub type GeneratorClientHandle = Arc<RwLock<TcpGeneratorClient>>;

pub async fn start_tcp_generator_client(
    app_tx: Sender<PGenAppUpdate>,
    controller_tx: Sender<PGenControllerCmd>,
    external_tx: Sender<ExternalJobCmd>,
    interface: TcpGeneratorInterface,
) -> anyhow::Result<Sender<GeneratorClientCmd>> {
    // Try initial connection first before spawning loop task
    let res = match interface {
        TcpGeneratorInterface::Resolve => resolve_connect_tcp_stream().await,
    };

    if let Err(e) = res {
        log::error!("{interface:?}: Failed connecting to TCP server: {e}");
        return Err(e.into());
    }

    let (client_tx, client_rx) = tokio::sync::mpsc::channel(5);
    let mut client_rx = ReceiverStream::new(client_rx).fuse();

    tokio::spawn(async move {
        let stream = res.unwrap();
        let mut client = TcpGeneratorClient {
            stream,
            interface,
            buf: vec![0; 512],
            running: true,
            controller_tx,
            external_tx,
        };

        loop {
            tokio::select! {
                Ok(_) = client.stream.readable() => {
                    if !client.running {
                        break;
                    }
                    if let Some(msg) = client.read_message().await {
                        client.try_send_pattern(&msg).await;
                    }
                }

                msg = client_rx.select_next_some() => {
                    log::trace!("{interface:?}: Received client command {msg:?}");
                    match msg {
                        GeneratorClientCmd::Shutdown => {
                            client.shutdown().await;
                            app_tx.try_send(PGenAppUpdate::GeneratorListening(false)).ok();
                            break;
                        },
                    }
                }
            }
        }
    });

    Ok(client_tx)
}

impl TcpGeneratorClient {
    pub async fn read_message(&mut self) -> Option<String> {
        let res = match self.interface {
            TcpGeneratorInterface::Resolve => {
                handle_resolve_tcp_stream_message(&mut self.stream).await
            }
        };

        match res {
            Ok(msg) => Some(msg),
            Err(e) => {
                self.handle_error(e).await;

                None
            }
        }
    }

    pub async fn try_send_pattern(&self, msg: &str) {
        match self.interface {
            TcpGeneratorInterface::Resolve => {
                handle_resolve_pattern_message(&self.controller_tx, msg).await;
            }
        }
    }

    pub async fn shutdown(&mut self) {
        self.stream.shutdown().await.ok();
        self.running = false;
    }

    async fn reconnect(&mut self) -> bool {
        self.shutdown().await;

        match self.interface {
            TcpGeneratorInterface::Resolve => {
                if let Ok(stream) = resolve_connect_tcp_stream().await {
                    self.stream = stream;
                    self.running = true;
                    return true;
                }
            }
        }

        log::error!("{:?}: Failed reconnecting TCP connection", self.interface);
        self.send_generator_stopped();

        false
    }

    async fn handle_error(&mut self, e: std::io::Error) {
        match e.kind() {
            ErrorKind::UnexpectedEof | ErrorKind::Other => {
                self.shutdown().await;
                self.send_generator_stopped();
            }
            ErrorKind::WouldBlock => {}
            _ => {
                log::error!("{e:?}");
                self.reconnect().await;
            }
        }
    }

    fn send_generator_stopped(&self) {
        let client = GeneratorInterface::Tcp(self.interface).client();
        self.external_tx
            .try_send(ExternalJobCmd::StopGeneratorClient(client))
            .ok();
    }
}
