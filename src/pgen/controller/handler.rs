use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};

use crate::app::PGenAppUpdate;
use crate::pgen::{
    client::{PGenClient, PGenTestPattern},
    commands::{PGenCommand, PGenCommandResponse, PGenInfoCommand},
    ColorFormat, DynamicRange,
};

use super::{PGenControllerContext, PGenControllerState};

pub type PGenControllerHandle = Arc<RwLock<PGenController>>;

#[derive(Debug)]
pub struct PGenController {
    pub ctx: PGenControllerContext,
    pub state: PGenControllerState,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct PGenOutputConfig {
    pub resolution: (u16, u16),
    pub format: ColorFormat,
    pub limited_range: bool,
    pub dynamic_range: DynamicRange,
}

impl PGenController {
    pub fn new(app_tx: Option<Sender<PGenAppUpdate>>) -> Self {
        let state = PGenControllerState::default();
        let client = Arc::new(Mutex::new(PGenClient::new(state.socket_addr)));

        let ctx = PGenControllerContext {
            client,
            app_tx,
            egui_ctx: Default::default(),
        };

        Self { ctx, state }
    }

    pub async fn set_initial_state(&mut self, state: PGenControllerState) {
        self.state = state;
        self.pgen_command(PGenCommand::UpdateSocket(self.state.socket_addr))
            .await;
    }

    pub fn update_ui(&self) {
        if let Some(egui_ctx) = self.ctx.egui_ctx.as_ref() {
            egui_ctx.request_repaint();
        }
    }

    pub fn handle_pgen_response(&mut self, res: PGenCommandResponse) {
        log::trace!("Received PGen command response: {:?}", res);

        let state_updated = !matches!(res, PGenCommandResponse::Busy | PGenCommandResponse::Ok(_));

        match res {
            PGenCommandResponse::NotConnected => self.state.connected_state.connected = false,
            PGenCommandResponse::Busy | PGenCommandResponse::Ok(_) => (),
            PGenCommandResponse::Errored(e) => self.state.connected_state.error = Some(e),
            PGenCommandResponse::Alive(is_alive) => self.state.connected_state.connected = is_alive,
            PGenCommandResponse::Connect(state)
            | PGenCommandResponse::Quit(state)
            | PGenCommandResponse::Shutdown(state)
            | PGenCommandResponse::Reboot(state) => self.state.connected_state = state,
            PGenCommandResponse::MultipleCommandInfo(res) => self.parse_commands_info(res),
        }

        if let Some(app_tx) = state_updated.then_some(self.ctx.app_tx.as_ref()).flatten() {
            app_tx
                .try_send(PGenAppUpdate::NewState(self.state.clone()))
                .ok();
            self.update_ui();
        }
    }

    pub async fn pgen_command(&mut self, cmd: PGenCommand) {
        log::trace!("Controller received command to execute: {:?}", cmd);

        let res = {
            let mut client = self.ctx.client.lock().await;
            client.send_generic_command(cmd).await
        };
        self.handle_pgen_response(res);
    }

    pub async fn update_socket(&mut self, socket_addr: SocketAddr) {
        self.state.socket_addr = socket_addr;
        self.pgen_command(PGenCommand::UpdateSocket(socket_addr))
            .await;
    }

    pub async fn send_heartbeat(&mut self) {
        if !self.state.connected_state.connected {
            return;
        }

        self.pgen_command(PGenCommand::IsAlive).await;
    }

    pub async fn fetch_output_info(&mut self) {
        self.pgen_command(PGenCommand::MultipleCommandsInfo(
            PGenInfoCommand::output_info_commands(),
        ))
        .await;
    }

    pub async fn initial_connect(&mut self) {
        self.pgen_command(PGenCommand::Connect).await;

        if self.state.connected_state.connected {
            self.fetch_output_info().await;
        }
    }

    pub async fn disconnect(&mut self) {
        if self.state.connected_state.connected {
            self.set_blank().await;
            self.pgen_command(PGenCommand::Quit).await;
        }
    }

    pub async fn reconnect(&mut self) {
        {
            let mut client = self.ctx.client.lock().await;

            // Don't auto connect
            if !client.connect_state.connected {
                return;
            }

            log::warn!("Reconnecting TCP socket stream");
            let res = client.set_stream().await;
            match res {
                Ok(_) => {
                    client.connect_state.connected = true;
                }
                Err(e) => client.connect_state.error = Some(e.to_string()),
            };
        }

        self.fetch_output_info().await;
    }

    pub fn get_color_format(&self) -> ColorFormat {
        self.state
            .output_config
            .as_ref()
            .map(|e| e.format)
            .unwrap_or_default()
    }

    pub async fn send_pattern(&mut self, pattern: PGenTestPattern) {
        // Only send non repeated patterns
        if self.state.pattern_config.patch_colour != pattern.rgb {
            self.state.pattern_config.patch_colour = pattern.rgb;
            self.state.pattern_config.background_colour = pattern.bg_rgb;
            self.pgen_command(PGenCommand::TestPattern(pattern)).await;
        }
    }

    pub async fn send_current_pattern(&mut self) {
        let pattern =
            PGenTestPattern::from_config(self.get_color_format(), &self.state.pattern_config);
        self.pgen_command(PGenCommand::TestPattern(pattern)).await;
    }

    pub async fn set_blank(&mut self) {
        let pattern = PGenTestPattern::blank(self.get_color_format(), &self.state.pattern_config);
        self.pgen_command(PGenCommand::TestPattern(pattern)).await;
    }

    pub fn parse_commands_info(&mut self, res: Vec<(PGenInfoCommand, String)>) {
        {
            let out_cfg = self
                .state
                .output_config
                .get_or_insert_with(Default::default);

            for (cmd, res) in res {
                match cmd {
                    PGenInfoCommand::GetResolution => {
                        if let Some(resolution) = PGenInfoCommand::parse_get_resolution(res) {
                            out_cfg.resolution = resolution;
                        }
                    }
                    PGenInfoCommand::GetColorFormat => {
                        if let Some(format) = cmd.parse_number_config::<u8>(res) {
                            out_cfg.format = ColorFormat::from(format);
                        }
                    }
                    PGenInfoCommand::GetOutputRange => {
                        if let Some(limited_range) = PGenInfoCommand::parse_get_output_range(res) {
                            out_cfg.limited_range = limited_range;
                            self.state.pattern_config.limited_range = limited_range;
                        }
                    }
                    PGenInfoCommand::GetOutputIsSDR => {
                        if cmd.parse_bool_config(res) {
                            out_cfg.dynamic_range = DynamicRange::Sdr;
                        }
                    }
                    PGenInfoCommand::GetOutputIsHDR => {
                        if cmd.parse_bool_config(res) {
                            out_cfg.dynamic_range = DynamicRange::Hdr10;
                        }
                    }
                    PGenInfoCommand::GetOutputIsLLDV => {
                        if cmd.parse_bool_config(res) {
                            out_cfg.dynamic_range = DynamicRange::LlDv;
                        }
                    }
                    PGenInfoCommand::GetOutputIsStdDovi => {
                        if cmd.parse_bool_config(res) {
                            out_cfg.dynamic_range = DynamicRange::StdDovi;
                        }
                    }
                }
            }

            log::debug!("Output config: {:?}", out_cfg);
        }

        self.state.set_pattern_size_and_pos_from_resolution();
    }
}
