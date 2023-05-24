use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use async_std::channel::{Receiver, Sender};
use eframe::egui;
use serde::{Deserialize, Serialize};

use super::{
    client::{ConnectState, PGenClient, PGenTestPattern},
    commands::{PGenCommand, PGenCommandResponse, PGenInfoCommand},
    pattern_config::PGenPatternConfig,
    scale_8b_rgb_to_10b, scale_rgb_into_range, ColorFormat, DynamicRange,
};

#[derive(Debug)]
pub struct PGenController {
    pub state: ControllerState,

    pub(crate) client: Arc<Mutex<PGenClient>>,

    cmd_sender: Sender<PGenCommandMsg>,
    state_receiver: Receiver<PGenCommandResponse>,

    // For waking up the UI thread
    pub(crate) egui_ctx: Option<egui::Context>,
}

pub struct PGenCommandMsg {
    pub client: Arc<Mutex<PGenClient>>,
    pub cmd: PGenCommand,

    // For waking up the UI thread
    pub egui_ctx: Option<egui::Context>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ControllerState {
    pub socket_addr: SocketAddr,
    pub editing_socket: (String, String),

    #[serde(skip)]
    pub connected_state: ConnectState,
    #[serde(skip)]
    pub output_config: Option<PGenOutputConfig>,

    pub pattern_config: PGenPatternConfig,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct PGenOutputConfig {
    pub resolution: (u16, u16),
    pub format: ColorFormat,
    pub limited_range: bool,
    pub dynamic_range: DynamicRange,
}

impl PGenController {
    pub fn new(
        cmd_sender: Sender<PGenCommandMsg>,
        state_receiver: Receiver<PGenCommandResponse>,
    ) -> Self {
        let state: ControllerState = Default::default();
        let client = PGenClient::new(state.socket_addr);

        Self {
            state,
            client: Arc::new(Mutex::new(client)),
            cmd_sender,
            state_receiver,
            egui_ctx: Default::default(),
        }
    }

    pub fn set_egui_context(&mut self, cc: &eframe::CreationContext) {
        self.egui_ctx = Some(cc.egui_ctx.clone());
    }

    pub fn restore_state(&mut self, state: ControllerState) {
        self.state = state;
    }

    pub fn has_messages_queued(&self) -> bool {
        !self.cmd_sender.is_empty() || !self.state_receiver.is_empty()
    }

    pub fn processing(&self) -> bool {
        self.has_messages_queued()
    }

    pub fn check_responses(&mut self) {
        while let Ok(res) = self.state_receiver.try_recv() {
            log::trace!("Received PGen command response: {:?}", res);

            match res {
                PGenCommandResponse::NotConnected => self.state.connected_state.connected = false,
                PGenCommandResponse::Busy | PGenCommandResponse::Ok(_) => (),
                PGenCommandResponse::Errored(e) => self.state.connected_state.error = Some(e),
                PGenCommandResponse::Alive(is_alive) => {
                    self.state.connected_state.connected = is_alive
                }
                PGenCommandResponse::Connect(state)
                | PGenCommandResponse::Quit(state)
                | PGenCommandResponse::Shutdown(state)
                | PGenCommandResponse::Reboot(state) => self.state.connected_state = state,

                PGenCommandResponse::MultipleCommandInfo(res) => self.parse_commands_info(res),
            }
        }

        if let Some(egui_ctx) = self.egui_ctx.as_ref() {
            egui_ctx.request_repaint();
        }
    }

    pub fn pgen_command(&self, cmd: PGenCommand) {
        let msg = PGenCommandMsg {
            client: self.client.clone(),
            cmd,
            egui_ctx: self.egui_ctx.as_ref().cloned(),
        };

        self.cmd_sender.try_send(msg).ok();
    }

    pub fn fetch_output_info(&self) {
        self.pgen_command(PGenCommand::MultipleCommandsInfo(
            PGenInfoCommand::output_info_commands(),
        ));
    }

    pub fn initial_connect(&self) {
        self.pgen_command(PGenCommand::Connect);
        self.fetch_output_info();
    }

    pub fn disconnect(&self) {
        if self.state.connected_state.connected {
            self.set_blank();
            self.pgen_command(PGenCommand::Quit);
        }
    }

    pub async fn reconnect(&mut self) {
        if let Ok(ref mut client) = self.client.lock() {
            // Don't auto connect
            if !client.connect_state.connected {
                return;
            }

            log::trace!("Reconnecting TCP socket stream");
            let res = client.set_stream().await;
            match res {
                Ok(_) => {
                    client.connect_state.connected = true;
                    self.fetch_output_info();
                }
                Err(e) => client.connect_state.error = Some(e.to_string()),
            };

            if let Some(egui_ctx) = self.egui_ctx.as_ref() {
                egui_ctx.request_repaint();
            }
        }
    }

    pub fn set_config_colour_from_8bit_srgb(&mut self, srgb_8bit: [u8; 3], background: bool) {
        let scaled_rgb = srgb_8bit.map(|c| {
            scale_8b_rgb_to_10b(
                c as u16,
                2.0,
                10,
                self.state.pattern_config.limited_range,
                false,
            )
        });

        if background {
            self.state.pattern_config.background_colour = scaled_rgb;
        } else {
            self.state.pattern_config.patch_colour = scaled_rgb;
        }
    }

    pub fn scale_rgb_values(&mut self, prev_depth: u8, prev_limited_range: bool) {
        let depth = self.state.pattern_config.bit_depth;
        let diff = depth.abs_diff(prev_depth) as f32;

        let limited_range = self.state.pattern_config.limited_range;

        if prev_depth == 8 {
            // 8 bit to 10 bit
            self.state
                .pattern_config
                .patch_colour
                .iter_mut()
                .chain(self.state.pattern_config.background_colour.iter_mut())
                .for_each(|c| {
                    *c = scale_8b_rgb_to_10b(*c, diff, depth, limited_range, prev_limited_range)
                });
        } else {
            // 10 bit to 8 bit
            self.state
                .pattern_config
                .patch_colour
                .iter_mut()
                .chain(self.state.pattern_config.background_colour.iter_mut())
                .for_each(|c| {
                    let mut val = *c as f32 / 2.0_f32.powf(diff);
                    val = scale_rgb_into_range(val, depth, limited_range, prev_limited_range);

                    *c = val.round() as u16;
                });
        }
    }

    pub fn get_color_format(&self) -> ColorFormat {
        self.state
            .output_config
            .as_ref()
            .map(|e| e.format)
            .unwrap_or_default()
    }

    pub fn send_pattern(&self) {
        let pattern =
            PGenTestPattern::from_config(self.get_color_format(), &self.state.pattern_config);
        self.pgen_command(PGenCommand::TestPattern(pattern));
    }

    pub fn set_blank(&self) {
        let pattern = PGenTestPattern::blank(self.get_color_format(), &self.state.pattern_config);
        self.pgen_command(PGenCommand::TestPattern(pattern));
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

            log::trace!("Output config: {:?}", out_cfg);
        }

        self.set_pattern_size_and_pos_from_resolution();
    }

    pub fn compute_max_pattern_size_and_position(&self) -> Option<(u16, u16, u16, u16)> {
        self.state.output_config.as_ref().map(|out_cfg| {
            let (max_w, max_h) = out_cfg.resolution;
            let (width, height) = self.state.pattern_config.patch_size;
            let (max_pos_x, max_pos_y) = (max_w - width, max_h - height);

            (max_w, max_h, max_pos_x, max_pos_y)
        })
    }

    pub fn set_pattern_size_and_pos_from_resolution(&mut self) {
        if let Some(out_cfg) = &self.state.output_config {
            let (width, height) = out_cfg.resolution;

            let patch_size = self
                .state
                .pattern_config
                .preset_size
                .patch_size_from_display_resolution(width, height);
            let position = self
                .state
                .pattern_config
                .preset_position
                .compute_position(width, height, patch_size);

            self.state.pattern_config.patch_size = patch_size;
            self.state.pattern_config.position = position;
        }
    }
}

impl Default for ControllerState {
    fn default() -> Self {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 85);

        Self {
            socket_addr,
            editing_socket: (socket_addr.ip().to_string(), socket_addr.port().to_string()),
            connected_state: Default::default(),
            output_config: Default::default(),
            pattern_config: Default::default(),
        }
    }
}
