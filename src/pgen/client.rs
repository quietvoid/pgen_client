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
    IsAlive,
    Connect,
    Quit,
    Shutdown,
    Reboot,
}

#[derive(Debug)]
pub enum PGenCommandResponse {
    NotConnected,
    Busy,
    Alive(bool),
    Connect(ConnectState),
    Quit(ConnectState),
    Shutdown(ConnectState),
    Reboot(ConnectState),
}

#[derive(Debug, Default, Clone)]
pub struct ConnectState {
    pub connected: bool,
    pub error: Option<String>,
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
        if self.stream.is_none() {
            return Ok(String::from("Not connected to TCP socket"));
        }

        let stream = self.stream.as_mut().unwrap();

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

    async fn send_heartbeat(&mut self) -> PGenCommandResponse {
        let is_alive = if let Ok(res) = self.send_tcp_command("IS_ALIVE").await {
            res == "ALIVE"
        } else {
            false
        };

        self.connect_state.connected = is_alive;

        PGenCommandResponse::Alive(is_alive)
    }

    pub async fn set_stream(&mut self) -> Result<(), io::Error> {
        if self.stream.is_some() {
            self.disconnect().await;
        }

        let stream = io::timeout(
            Duration::from_secs(10),
            TcpStream::connect(self.socket_addr),
        )
        .await?;
        self.stream = Some(stream);

        let stream = self.stream.as_mut().unwrap();
        log::info!("Connected to {}", &stream.peer_addr()?);

        Ok(())
    }

    async fn connect(&mut self) -> PGenCommandResponse {
        self.connect_state.error = None;

        let res: Result<bool, io::Error> = task::block_on(async {
            if !self.connect_state.connected {
                self.set_stream().await?;
            } else {
                log::info!("Already connected, requesting heartbeat");
            }

            let PGenCommandResponse::Alive(is_alive) = self.send_heartbeat().await else {
                unreachable!()
            };

            Ok(is_alive)
        });

        match &res {
            Ok(res) => self.connect_state.connected = *res,
            Err(e) => self.connect_state.error = Some(e.to_string()),
        };

        PGenCommandResponse::Connect(self.connect_state.clone())
    }

    async fn disconnect(&mut self) -> PGenCommandResponse {
        let res: Result<bool, io::Error> = task::block_on(async {
            let connected = if self.connect_state.connected {
                let res = self.send_tcp_command("QUIT").await?;

                !res.is_empty()
            } else {
                log::info!("Already disconnected");
                false
            };

            Ok(connected)
        });

        match &res {
            Ok(still_connected) => {
                self.connect_state.connected = *still_connected;

                if *still_connected {
                    log::debug!("Failed disconnecting connection");
                } else {
                    self.stream = None;
                }
            }
            Err(e) => self.connect_state.error = Some(e.to_string()),
        };

        PGenCommandResponse::Quit(self.connect_state.clone())
    }

    async fn shutdown_device(&mut self) -> PGenCommandResponse {
        let res: Result<bool, io::Error> = task::block_on(async {
            let res = self.send_tcp_command("CMD:HALT").await?;
            Ok(res == "OK:")
        });

        match &res {
            Ok(res) => {
                if *res {
                    self.connect_state.connected = false;
                    self.stream = None;
                }
            }
            Err(e) => self.connect_state.error = Some(e.to_string()),
        };

        PGenCommandResponse::Shutdown(self.connect_state.clone())
    }

    async fn reboot_device(&mut self) -> PGenCommandResponse {
        let res: Result<bool, io::Error> = task::block_on(async {
            let res = self.send_tcp_command("CMD:REBOOT").await?;
            Ok(res == "OK:")
        });

        match &res {
            Ok(res) => {
                if *res {
                    self.connect_state.connected = false;
                    self.stream = None;
                }
            }
            Err(e) => self.connect_state.error = Some(e.to_string()),
        };

        PGenCommandResponse::Reboot(self.connect_state.clone())
    }

    pub async fn send_generic_command(&mut self, cmd: PGenCommand) -> PGenCommandResponse {
        if self.stream.is_none() && !matches!(cmd, PGenCommand::Connect) {
            return PGenCommandResponse::NotConnected;
        }

        match cmd {
            PGenCommand::IsAlive => self.send_heartbeat().await,
            PGenCommand::Connect => self.connect().await,
            PGenCommand::Quit => self.disconnect().await,
            PGenCommand::Shutdown => self.shutdown_device().await,
            PGenCommand::Reboot => self.reboot_device().await,
        }
    }
}
