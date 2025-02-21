use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use itertools::Itertools;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::utils::{Rgb, compute_rgb_range};

use super::ColorFormat;
use super::commands::{PGenCommand, PGenCommandResponse, PGenGetConfCommand, PGenSetConfCommand};
use super::pattern_config::PGenPatternConfig;

const PGEN_CMD_END_BYTE_STR: &str = "\x02\x0D";
const PGEN_CMD_END_BYTES: &[u8] = PGEN_CMD_END_BYTE_STR.as_bytes();

#[derive(Debug)]
pub struct PGenClient {
    stream: Option<TcpStream>,
    socket_addr: SocketAddr,
    response_buffer: Vec<u8>,

    pub connect_state: ConnectState,
}

#[derive(Debug, Default, Clone)]
pub struct ConnectState {
    pub connected: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PGenTestPattern {
    pub format: ColorFormat,

    pub position: (u16, u16),
    pub patch_size: (u16, u16),

    pub bit_depth: u8,

    // In 10 bit range
    pub rgb: Rgb,
    pub bg_rgb: Rgb,
}

impl PGenClient {
    pub fn new(socket_addr: SocketAddr) -> Self {
        Self {
            stream: None,
            socket_addr,
            response_buffer: vec![0; 8192],
            connect_state: Default::default(),
        }
    }

    pub fn set_socket_address(&mut self, socket_addr: &SocketAddr) {
        self.connect_state.connected = false;
        self.socket_addr.set_ip(socket_addr.ip());
        self.socket_addr.set_port(socket_addr.port());
    }

    pub async fn update_socket_address_and_connect(
        &mut self,
        socket_addr: &SocketAddr,
    ) -> PGenCommandResponse {
        self.set_socket_address(socket_addr);
        self.connect().await
    }

    async fn send_tcp_command(&mut self, cmd: &str) -> Result<String> {
        if self.stream.is_none() {
            return Ok(String::from("Not connected to TCP socket"));
        }

        log::debug!("Sending command {}", cmd);

        let stream = self.stream.as_mut().unwrap();
        stream
            .write_all(format!("{cmd}{PGEN_CMD_END_BYTE_STR}").as_bytes())
            .await?;

        let res_bytes = timeout(Duration::from_secs(10), async {
            let mut n = 0;
            let mut correct_end_bytes = false;

            while let Ok(read_bytes) = stream.read(&mut self.response_buffer[n..]).await {
                n += read_bytes;
                if read_bytes == 0 {
                    break;
                } else {
                    correct_end_bytes =
                        matches!(&self.response_buffer[n - 2..n], PGEN_CMD_END_BYTES);
                    if correct_end_bytes {
                        break;
                    }
                }
            }

            if correct_end_bytes {
                &self.response_buffer[..n - 2]
            } else {
                &self.response_buffer[..n]
            }
        })
        .await?;

        let response = String::from_utf8_lossy(res_bytes).to_string();
        log::trace!("  Response: {response}");

        Ok(response)
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

    pub async fn set_stream(&mut self) -> Result<()> {
        if self.stream.is_some() {
            self.disconnect().await;
        }

        let stream = timeout(
            Duration::from_secs(10),
            TcpStream::connect(self.socket_addr),
        )
        .await??;
        self.stream = Some(stream);

        let stream = self.stream.as_mut().unwrap();
        log::info!("Successfully connected to {}", &stream.peer_addr()?);

        Ok(())
    }

    async fn connect(&mut self) -> PGenCommandResponse {
        self.connect_state.connected = false;
        self.connect_state.error = None;
        log::info!("Connecting to {}", self.socket_addr);

        let res = if !self.connect_state.connected {
            self.set_stream().await
        } else {
            log::trace!("Already connected, requesting heartbeat");
            Ok(())
        };

        let PGenCommandResponse::Alive(is_alive) = self.send_heartbeat().await else {
            unreachable!()
        };

        self.connect_state.connected = is_alive;
        if let Err(e) = res {
            self.connect_state.error = Some(e.to_string());
        }

        PGenCommandResponse::Connect(self.connect_state.clone())
    }

    async fn disconnect(&mut self) -> PGenCommandResponse {
        let res = if self.connect_state.connected {
            self.send_tcp_command("QUIT")
                .await
                .map(|res| !res.is_empty())
        } else {
            log::trace!("Already disconnected");
            Ok(false)
        };

        match res {
            Ok(still_connected) => {
                self.connect_state.connected = still_connected;

                if still_connected {
                    log::error!("Failed disconnecting connection");
                } else {
                    self.stream = None;
                }
            }
            Err(e) => self.connect_state.error = Some(e.to_string()),
        };

        PGenCommandResponse::Quit(self.connect_state.clone())
    }

    async fn shutdown_device(&mut self) -> PGenCommandResponse {
        let res = self
            .send_tcp_command("CMD:HALT")
            .await
            .map(|res| res == "OK:");

        match res {
            Ok(res) => {
                if res {
                    self.connect_state.connected = false;
                    self.stream = None;
                }
            }
            Err(e) => self.connect_state.error = Some(e.to_string()),
        };

        PGenCommandResponse::Shutdown(self.connect_state.clone())
    }

    async fn reboot_device(&mut self) -> PGenCommandResponse {
        let res = self
            .send_tcp_command("CMD:REBOOT")
            .await
            .map(|res| res == "OK:");

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

    async fn restart_software(&mut self) -> PGenCommandResponse {
        PGenCommandResponse::Ok(self.send_tcp_command("RESTARTPGENERATOR:").await.is_ok())
    }

    async fn send_test_pattern(&mut self, test_pattern: &PGenTestPattern) -> PGenCommandResponse {
        let rect = if test_pattern.bit_depth == 8 {
            "RECTANGLE"
        } else {
            "RECTANGLE10bit"
        };

        let (w, h) = test_pattern.patch_size;
        let (x, y) = test_pattern.position;
        let mut bg_rgb = test_pattern.bg_rgb;
        let [r, g, b] = test_pattern.rgb;

        // Only RGB supports 10 bit backgrounds, 8 bit otherwise
        if test_pattern.bit_depth == 10 && !matches!(test_pattern.format, ColorFormat::Rgb) {
            bg_rgb.iter_mut().for_each(|c| {
                let cf = *c as f64 / 2.0_f64.powf(2.0);
                *c = cf.round() as u16;
            });
        }

        let [bg_r, bg_b, bg_g] = bg_rgb;

        let cmd = format!("RGB={rect};{w},{h};0;{r},{g},{b};{bg_r},{bg_b},{bg_g};0,0,{x},{y};-1");

        log::info!("Sent pattern RGB: [{r}, {g}, {b}], background: [{bg_r}, {bg_g}, {bg_b}]");
        PGenCommandResponse::Ok(self.send_tcp_command(&cmd).await.is_ok())
    }

    pub async fn send_multiple_get_conf_commands(
        &mut self,
        commands: &[PGenGetConfCommand],
    ) -> PGenCommandResponse {
        let commands_str = commands.iter().map(|c| c.as_ref()).join(":");
        let cmd = format!("CMD:MULTIPLE:{commands_str}");

        let res = self.send_tcp_command(&cmd).await;

        match res {
            Ok(res) => {
                // Skip `OK:\n`
                let command_results = res.split_terminator('\n').skip(1);
                let paired_results = command_results
                    .zip(commands.iter().copied())
                    .map(|(res, c)| (c, res.to_owned()))
                    .collect();
                PGenCommandResponse::MultipleGetConfRes(paired_results)
            }
            Err(e) => {
                let err_str = e.to_string();
                self.connect_state.error = Some(err_str.clone());
                PGenCommandResponse::Errored(err_str)
            }
        }
    }

    pub async fn send_multiple_set_conf_commands(
        &mut self,
        commands: &[PGenSetConfCommand],
    ) -> PGenCommandResponse {
        let mut ret = Vec::with_capacity(commands.len());

        for cmd in commands.iter().copied() {
            let cmd_str = format!("CMD:{}:{}", cmd.as_ref(), cmd.value());
            let res = self
                .send_tcp_command(&cmd_str)
                .await
                .map(|res| res == "OK:");
            match res {
                Ok(res) => ret.push((cmd, res)),
                Err(e) => {
                    let err_str = e.to_string();
                    self.connect_state.error = Some(err_str.clone());
                    return PGenCommandResponse::Errored(err_str);
                }
            }
        }

        PGenCommandResponse::MultipleSetConfRes(ret)
    }

    pub async fn send_generic_command(&mut self, cmd: PGenCommand) -> PGenCommandResponse {
        if self.stream.is_none() && !matches!(cmd, PGenCommand::Connect) {
            match cmd {
                PGenCommand::UpdateSocket(socket_addr) => {
                    self.set_socket_address(&socket_addr);
                    return PGenCommandResponse::Ok(true);
                }
                _ => return PGenCommandResponse::NotConnected,
            }
        }

        match cmd {
            PGenCommand::IsAlive => self.send_heartbeat().await,
            PGenCommand::Connect => self.connect().await,
            PGenCommand::Quit => self.disconnect().await,
            PGenCommand::Shutdown => self.shutdown_device().await,
            PGenCommand::Reboot => self.reboot_device().await,
            PGenCommand::RestartSoftware => self.restart_software().await,
            PGenCommand::UpdateSocket(socket_addr) => {
                self.update_socket_address_and_connect(&socket_addr).await
            }
            PGenCommand::TestPattern(pattern) => self.send_test_pattern(&pattern).await,
            PGenCommand::MultipleGetConfCommands(commands) => {
                self.send_multiple_get_conf_commands(commands).await
            }
            PGenCommand::MultipleSetConfCommands(commands) => {
                self.send_multiple_set_conf_commands(&commands).await
            }
        }
    }
}

impl PGenTestPattern {
    pub fn from_config(format: ColorFormat, cfg: &PGenPatternConfig) -> Self {
        Self {
            format,
            position: cfg.position,
            patch_size: cfg.patch_size,
            bit_depth: cfg.bit_depth as u8,
            rgb: cfg.patch_colour,
            bg_rgb: cfg.background_colour,
        }
    }

    pub fn blank(format: ColorFormat, cfg: &PGenPatternConfig) -> Self {
        let rgb_range = compute_rgb_range(cfg.limited_range, cfg.bit_depth as u8);
        let rgb = [*rgb_range.start(); 3];
        let bg_rgb = rgb;

        Self {
            format,
            position: cfg.position,
            patch_size: cfg.patch_size,
            bit_depth: cfg.bit_depth as u8,
            rgb,
            bg_rgb,
        }
    }
}
