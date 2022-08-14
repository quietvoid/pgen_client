use std::net::SocketAddr;
use std::time::Duration;

use async_std::io;
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::task;

const PGEN_CMD_END_BYTE_STR: &str = "\x02\x0D";
const PGEN_CMD_END_BYTES: &[u8] = PGEN_CMD_END_BYTE_STR.as_bytes();

pub struct PGenClient {
    stream: Option<TcpStream>,
    socket_addr: SocketAddr,

    pub connect_state: ConnectState,
}

#[derive(Debug)]
pub enum PGenCommand {
    Connect,
}

#[derive(Debug)]
pub enum PGenCommandResponse {
    Busy,
    Connect(ConnectState),
}

#[derive(Debug, Default, Clone)]
pub struct ConnectState {
    pub connected: bool,
    pub connect_error: Option<String>,
}

impl PGenClient {
    pub fn new(socket_addr: SocketAddr) -> Self {
        Self {
            stream: None,
            socket_addr,
            connect_state: Default::default(),
        }
    }

    fn clean_response(bytes: &[u8]) -> &[u8] {
        let end_bytes_idx = bytes.windows(2).position(|w| w == PGEN_CMD_END_BYTES);

        if let Some(idx) = end_bytes_idx {
            &bytes[..idx]
        } else {
            bytes
        }
    }

    async fn send_tcp_command(&mut self, cmd: &str) -> io::Result<String> {
        let stream = self.stream.as_mut().expect("Missing TCP socket stream");

        log::debug!("Sending command {}", cmd);

        stream
            .write_fmt(format_args!("{cmd}{PGEN_CMD_END_BYTE_STR}"))
            .await?;

        io::timeout(Duration::from_secs(10), async move {
            let mut buf = vec![0u8; 1024];
            let n = stream.read(&mut buf).await?;

            let res_bytes = Self::clean_response(&buf[..n]);

            let response = String::from_utf8_lossy(res_bytes).to_string();
            log::debug!("  {} response: {}", cmd, response);

            Ok(response)
        })
        .await
    }

    async fn connect(&mut self) -> PGenCommandResponse {
        let res: Result<bool, io::Error> = task::block_on(async {
            if !self.connect_state.connected {
                let stream = io::timeout(
                    Duration::from_secs(10),
                    TcpStream::connect(self.socket_addr),
                )
                .await?;
                self.stream = Some(stream);

                let stream = self.stream.as_mut().expect("Missing TCP socket stream");
                log::info!("Connected to {}", &stream.peer_addr()?);
            } else {
                log::info!("Already connected, requesting heartbeat");
            }

            let res = self.send_tcp_command("IS_ALIVE").await?;
            let is_alive = res == "ALIVE";

            Ok(is_alive)
        });

        match &res {
            Ok(res) => self.connect_state.connected = *res,
            Err(e) => self.connect_state.connect_error = Some(e.to_string()),
        };

        PGenCommandResponse::Connect(self.connect_state.clone())
    }

    pub async fn send_generic_command(&mut self, cmd: PGenCommand) -> PGenCommandResponse {
        match cmd {
            PGenCommand::Connect => self.connect().await,
        }
    }
}
