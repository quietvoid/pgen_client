use std::{
    io::{BufReader, ErrorKind, Read},
    net::{Shutdown, TcpStream},
    sync::Arc,
    time::Duration,
};

use async_std::{sync::Mutex, task};
use async_stream::stream;
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::pgen::interfaces::resolve::resolve_connect_and_set_tcp_stream;

pub mod resolve;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub struct GeneratorInfo {
    pub interface: GeneratorInterface,
    pub listening: bool,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub enum GeneratorInterface {
    #[default]
    Resolve,
}

pub struct TcpGeneratorClient {
    pub stream: Option<BufReader<TcpStream>>,
    pub interface: GeneratorInterface,
    pub buf: Vec<u8>,
}
pub type GeneratorClientHandle = Arc<Mutex<TcpGeneratorClient>>;

impl TcpGeneratorClient {
    pub fn get_stream(client: GeneratorClientHandle) -> impl Stream<Item = String> {
        stream! {
            loop {
                task::sleep(Duration::from_millis(5)).await;

                let mut res = None;
                let mut err = None;
                if let Some(mut client) = client.try_lock() {
                    if let Some(tcp_stream) = client.stream.as_mut() {
                        let mut header = [0; 4];

                        if let Err(e) = tcp_stream.read_exact(&mut header) {
                            log::trace!("Error in TCP connection: {e}");
                            err.replace(e);
                        } else {
                            let msg_len = u32::from_be_bytes(header) as usize;
                            let mut msg = vec![0_u8; msg_len];
                            tcp_stream.read_exact(msg.as_mut()).ok();

                            if let Ok(msg_str) = String::from_utf8(msg) {
                                res.replace(msg_str);
                            }
                        }
                    }
                }

                if let Some(res) = res {
                    yield res;
                } else if let Some(e) = err {
                    Self::handle_error(e.kind(), client.clone()).await;
                }
            }
        }
    }

    async fn handle_error(error_kind: ErrorKind, client: GeneratorClientHandle) {
        match error_kind {
            ErrorKind::UnexpectedEof => {
                let mut client = client.lock().await;
                let _ = client.stream.take();
            }
            _ => {
                let interface = {
                    let mut client = client.lock().await;
                    let reader = client.stream.take().unwrap();
                    let stream = reader.into_inner();
                    stream.shutdown(Shutdown::Both).ok();

                    client.interface
                };

                // Try reconnecting
                match interface {
                    GeneratorInterface::Resolve => {
                        resolve_connect_and_set_tcp_stream(client.clone()).await;
                    }
                };
            }
        }
    }
}
