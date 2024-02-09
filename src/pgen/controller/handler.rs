use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

use crate::app::PGenAppUpdate;
use crate::pgen::commands::PGenSetConfCommand;
use crate::pgen::pattern_config::PGenPatternConfig;
use crate::pgen::{
    client::{PGenClient, PGenTestPattern},
    commands::{PGenCommand, PGenCommandResponse, PGenGetConfCommand},
    ColorFormat, DynamicRange,
};
use crate::pgen::{BitDepth, Colorimetry, HdrEotf, Primaries, QuantRange};
use crate::utils::scale_pattern_config_rgb_values;

use super::{DisplayMode, PGenControllerContext, PGenControllerState};

pub type PGenControllerHandle = Arc<Mutex<PGenController>>;

#[derive(Debug)]
pub struct PGenController {
    pub ctx: PGenControllerContext,
    pub state: PGenControllerState,
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

        let res = {
            let mut client = self.ctx.client.lock().await;
            client
                .send_generic_command(PGenCommand::UpdateSocket(self.state.socket_addr))
                .await
        };

        self.handle_pgen_response(res);
    }

    pub fn update_ui(&self) {
        if let Some(egui_ctx) = self.ctx.egui_ctx.as_ref() {
            egui_ctx.request_repaint();
        }
    }

    pub fn try_update_app_state(&self, state_updated: bool) {
        if let Some(app_tx) = state_updated.then_some(self.ctx.app_tx.as_ref()).flatten() {
            app_tx
                .try_send(PGenAppUpdate::NewState(self.state.clone()))
                .ok();
            self.update_ui();
        }
    }

    pub fn handle_pgen_response(&mut self, res: PGenCommandResponse) {
        let mut state_updated =
            !matches!(res, PGenCommandResponse::Busy | PGenCommandResponse::Ok(_));
        if let PGenCommandResponse::Alive(is_alive) = res {
            state_updated = is_alive != self.state.connected_state.connected;
        };

        match res {
            PGenCommandResponse::NotConnected => self.state.connected_state.connected = false,
            PGenCommandResponse::Busy | PGenCommandResponse::Ok(_) => (),
            PGenCommandResponse::Errored(e) => self.state.connected_state.error = Some(e),
            PGenCommandResponse::Alive(is_alive) => self.state.connected_state.connected = is_alive,
            PGenCommandResponse::Connect(state)
            | PGenCommandResponse::Quit(state)
            | PGenCommandResponse::Shutdown(state)
            | PGenCommandResponse::Reboot(state) => self.state.connected_state = state,
            PGenCommandResponse::MultipleGetConfRes(res) => {
                self.parse_multiple_get_conf_commands_res(res);
            }
            PGenCommandResponse::MultipleSetConfRes(res) => {
                self.parse_multiple_set_conf_commands_res(&res);
            }
        }

        self.try_update_app_state(state_updated);
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

    pub async fn initial_connect(&mut self) {
        self.pgen_command(PGenCommand::Connect).await;
        self.fetch_base_info().await;
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

        self.fetch_base_info().await;
    }

    pub async fn fetch_base_info(&mut self) {
        if self.state.connected_state.connected {
            self.pgen_command(PGenCommand::MultipleGetConfCommands(
                PGenGetConfCommand::base_info_commands(),
            ))
            .await;
        }
    }

    pub async fn restart_pgenerator_software(&mut self, refetch: bool) {
        self.pgen_command(PGenCommand::RestartSoftware).await;
        self.set_blank().await;

        if refetch {
            self.fetch_base_info().await;
        } else {
            self.update_pgenerator_pid().await;
        }
    }

    async fn update_pgenerator_pid(&mut self) {
        self.pgen_command(PGenCommand::MultipleGetConfCommands(&[
            PGenGetConfCommand::GetPGeneratorPid,
        ]))
        .await;
    }

    pub async fn change_display_mode(&mut self, mode: DisplayMode, get_pid: bool) {
        self.pgen_command(PGenCommand::MultipleSetConfCommands(vec![
            PGenSetConfCommand::SetDisplayMode(mode),
        ]))
        .await;

        if get_pid {
            // Setting display mode restarts the PGenerator, get new PID
            self.update_pgenerator_pid().await;
        }

        self.set_blank().await;
    }

    async fn set_dolby_vision_mode(&mut self) {
        // Try finding a 1080p@60 display mode
        let valid_display_mode = self.state.pgen_info.as_ref().and_then(|info| {
            info.display_modes
                .iter()
                .find(|mode| mode.resolution == (1920, 1080) && mode.refresh_rate == 60.0)
        });

        if let Some(valid_mode) = valid_display_mode.copied() {
            let needs_mode_switch = self
                .state
                .pgen_info
                .as_ref()
                .is_some_and(|info| info.current_display_mode != valid_mode);

            // Dolby Vision requires 8 bit patches
            self.state.pattern_config.bit_depth = BitDepth::Eight;

            if needs_mode_switch {
                self.change_display_mode(valid_mode, false).await;
            }

            // Set Dolby Vision configs
            let commands = PGenSetConfCommand::commands_for_dynamic_range(DynamicRange::Dovi);
            self.pgen_command(PGenCommand::MultipleSetConfCommands(commands))
                .await;

            // Restart for changes to apply
            self.restart_pgenerator_software(false).await;
            self.try_update_app_state(true);
        } else {
            log::error!("Cannot set Dolby Vision, no 1080p display mode found");
        }
    }

    pub async fn update_dynamic_range(&mut self, dynamic_range: DynamicRange) {
        if dynamic_range == DynamicRange::Dovi {
            self.set_dolby_vision_mode().await;
        } else {
            let commands = PGenSetConfCommand::commands_for_dynamic_range(dynamic_range);
            self.pgen_command(PGenCommand::MultipleSetConfCommands(commands))
                .await;
            self.restart_pgenerator_software(false).await;
        }
    }

    pub fn get_color_format(&self) -> ColorFormat {
        self.state
            .pgen_info
            .as_ref()
            .map(|e| e.output_config.format)
            .unwrap_or_default()
    }

    pub async fn send_pattern_from_cfg(&mut self, config: PGenPatternConfig) {
        // Only send non repeated patterns
        if self.state.pattern_config.patch_colour != config.patch_colour {
            let mut new_pattern_cfg = PGenPatternConfig {
                bit_depth: config.bit_depth,
                patch_colour: config.patch_colour,
                background_colour: config.background_colour,
                ..self.state.pattern_config
            };

            if self.state.is_dovi_mode() && new_pattern_cfg.bit_depth != BitDepth::Eight {
                // Ensure DoVi patterns are 8 bit
                let prev_depth = new_pattern_cfg.bit_depth as u8;

                scale_pattern_config_rgb_values(&mut new_pattern_cfg, 8, prev_depth, false, false);
            }

            // Update current pattern and send it
            self.state.pattern_config = new_pattern_cfg;
            self.try_update_app_state(true);

            self.send_current_pattern().await;
        }
    }

    pub async fn send_current_pattern(&mut self) {
        let pattern =
            PGenTestPattern::from_config(self.get_color_format(), &self.state.pattern_config);
        self.pgen_command(PGenCommand::TestPattern(pattern)).await;
    }

    pub async fn set_blank(&mut self) {
        let mut config = self.state.pattern_config;
        config.patch_colour = Default::default();
        config.background_colour = Default::default();

        self.send_pattern_from_cfg(config).await;
    }

    pub async fn send_pattern_and_wait(&mut self, config: PGenPatternConfig) {
        self.send_pattern_from_cfg(config).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    pub fn parse_multiple_get_conf_commands_res(&mut self, res: Vec<(PGenGetConfCommand, String)>) {
        let pgen_info = self.state.pgen_info.get_or_insert_with(Default::default);
        let out_cfg = &mut pgen_info.output_config;
        let mut changed_mode = false;

        for (cmd, res) in res {
            match cmd {
                PGenGetConfCommand::GetPGeneratorVersion => {
                    pgen_info.version = cmd.parse_string_config(res.as_str()).to_owned();
                }
                PGenGetConfCommand::GetPGeneratorPid => {
                    pgen_info.pid = cmd.parse_string_config(res.as_str()).to_owned();
                }
                PGenGetConfCommand::GetCurrentMode => {
                    if let Ok(mode) =
                        DisplayMode::try_from_str(cmd.parse_string_config(res.as_str()))
                    {
                        pgen_info.current_display_mode = mode;
                        changed_mode = true;
                    }
                }
                PGenGetConfCommand::GetModesAvailable => {
                    let decoded_str = STANDARD
                        .decode(cmd.parse_string_config(res.as_str()))
                        .ok()
                        .and_then(|e| String::from_utf8(e).ok());
                    if let Some(modes_str) = decoded_str {
                        pgen_info.display_modes = modes_str
                            .lines()
                            .map(DisplayMode::try_from_str)
                            .filter_map(Result::ok)
                            .collect();
                    }
                }
                PGenGetConfCommand::GetColorFormat => {
                    if let Some(format) = cmd
                        .parse_number_config::<usize>(res)
                        .and_then(ColorFormat::from_repr)
                    {
                        out_cfg.format = format;
                    }
                }
                PGenGetConfCommand::GetBitDepth => {
                    if let Some(bit_depth) = cmd
                        .parse_number_config::<usize>(res)
                        .and_then(BitDepth::from_repr)
                    {
                        out_cfg.bit_depth = bit_depth;
                    }
                }
                PGenGetConfCommand::GetQuantRange => {
                    if let Some(quant_range) = cmd
                        .parse_number_config::<usize>(res)
                        .and_then(QuantRange::from_repr)
                    {
                        out_cfg.quant_range = quant_range;
                        self.state.pattern_config.limited_range =
                            quant_range == QuantRange::Limited;
                    }
                }
                PGenGetConfCommand::GetColorimetry => {
                    if let Some(colorimetry) = cmd
                        .parse_number_config::<usize>(res)
                        .and_then(Colorimetry::from_repr)
                    {
                        out_cfg.colorimetry = colorimetry;
                    }
                }
                PGenGetConfCommand::GetOutputRange => {
                    if let Some(limited_range) = PGenGetConfCommand::parse_get_output_range(res) {
                        out_cfg.quant_range = QuantRange::from(limited_range);
                    }
                }
                PGenGetConfCommand::GetOutputIsSDR => {
                    if cmd.parse_bool_config(res) {
                        out_cfg.dynamic_range = DynamicRange::Sdr;
                    }
                }
                PGenGetConfCommand::GetOutputIsHDR => {
                    if cmd.parse_bool_config(res) {
                        out_cfg.dynamic_range = DynamicRange::Hdr;
                    }
                }
                PGenGetConfCommand::GetOutputIsLLDV => {
                    if cmd.parse_bool_config(res) {
                        out_cfg.dynamic_range = DynamicRange::Dovi;
                    }
                }
                PGenGetConfCommand::GetOutputIsStdDovi => {
                    if cmd.parse_bool_config(res) {
                        out_cfg.dynamic_range = DynamicRange::Dovi;
                    }
                }
                PGenGetConfCommand::GetHdrEotf => {
                    if let Some(eotf) = cmd
                        .parse_number_config::<usize>(res)
                        .and_then(HdrEotf::from_repr)
                    {
                        out_cfg.hdr_meta.eotf = eotf;
                    }
                }
                PGenGetConfCommand::GetHdrPrimaries => {
                    if let Some(primaries) = cmd
                        .parse_number_config::<usize>(res)
                        .and_then(Primaries::from_repr)
                    {
                        out_cfg.hdr_meta.primaries = primaries;
                    }
                }
                PGenGetConfCommand::GetHdrMaxMdl => {
                    if let Some(max_mdl) = cmd.parse_number_config::<u16>(res) {
                        out_cfg.hdr_meta.max_mdl = max_mdl;
                    }
                }
                PGenGetConfCommand::GetHdrMinMdl => {
                    if let Some(min_mdl) = cmd.parse_number_config::<u16>(res) {
                        out_cfg.hdr_meta.min_mdl = min_mdl;
                    }
                }
                PGenGetConfCommand::GetHdrMaxCLL => {
                    if let Some(maxcll) = cmd.parse_number_config::<u16>(res) {
                        out_cfg.hdr_meta.maxcll = maxcll;
                    }
                }
                PGenGetConfCommand::GetHdrMaxFALL => {
                    if let Some(maxfall) = cmd.parse_number_config::<u16>(res) {
                        out_cfg.hdr_meta.maxfall = maxfall;
                    }
                }
            }
        }

        log::trace!("PGenerator info: {:?}", self.state.pgen_info);

        if changed_mode {
            self.state.set_pattern_size_and_pos_from_resolution();
        }
    }

    pub async fn send_multiple_set_conf_commands<'a>(&mut self, commands: Vec<PGenSetConfCommand>) {
        self.pgen_command(PGenCommand::MultipleSetConfCommands(commands))
            .await;
    }

    pub fn parse_multiple_set_conf_commands_res(&mut self, res: &[(PGenSetConfCommand, bool)]) {
        let mut changed_mode = false;

        if let Some(pgen_info) = self.state.pgen_info.as_mut() {
            let successful_sets = res.iter().filter_map(|(cmd, ok)| ok.then_some(*cmd));

            for cmd in successful_sets {
                match cmd {
                    PGenSetConfCommand::SetDisplayMode(mode) => {
                        pgen_info.current_display_mode = mode;
                        changed_mode = true;
                    }
                    PGenSetConfCommand::SetColorFormat(format) => {
                        pgen_info.output_config.format = format;
                    }
                    PGenSetConfCommand::SetBitDepth(bit_depth) => {
                        pgen_info.output_config.bit_depth = bit_depth;
                    }
                    PGenSetConfCommand::SetQuantRange(quant_range) => {
                        pgen_info.output_config.quant_range = quant_range;
                        self.state.pattern_config.limited_range =
                            quant_range == QuantRange::Limited;
                    }
                    PGenSetConfCommand::SetColorimetry(colorimetry) => {
                        pgen_info.output_config.colorimetry = colorimetry;
                    }
                    PGenSetConfCommand::SetOutputIsSDR(is_sdr) => {
                        if is_sdr {
                            pgen_info.output_config.dynamic_range = DynamicRange::Sdr;
                        }
                    }
                    PGenSetConfCommand::SetOutputIsHDR(is_hdr) => {
                        if is_hdr {
                            pgen_info.output_config.dynamic_range = DynamicRange::Hdr;
                        }
                    }
                    PGenSetConfCommand::SetOutputIsLLDV(is_lldv) => {
                        if is_lldv {
                            pgen_info.output_config.dynamic_range = DynamicRange::Dovi;
                        }
                    }
                    PGenSetConfCommand::SetOutputIsStdDovi(is_std_dovi) => {
                        if is_std_dovi {
                            pgen_info.output_config.dynamic_range = DynamicRange::Dovi;
                        }
                    }
                    PGenSetConfCommand::SetDoviStatus(_)
                    | PGenSetConfCommand::SetDoviInterface(_) => {}
                    PGenSetConfCommand::SetDoviMapMode(dovi_map_mode) => {
                        pgen_info.output_config.dovi_map_mode = dovi_map_mode;
                    }
                    PGenSetConfCommand::SetHdrEotf(eotf) => {
                        pgen_info.output_config.hdr_meta.eotf = eotf;
                    }
                    PGenSetConfCommand::SetHdrPrimaries(primaries) => {
                        pgen_info.output_config.hdr_meta.primaries = primaries;
                    }
                    PGenSetConfCommand::SetHdrMaxMdl(max_mdl) => {
                        pgen_info.output_config.hdr_meta.max_mdl = max_mdl;
                    }
                    PGenSetConfCommand::SetHdrMinMdl(min_mdl) => {
                        pgen_info.output_config.hdr_meta.min_mdl = min_mdl;
                    }
                    PGenSetConfCommand::SetHdrMaxCLL(maxcll) => {
                        pgen_info.output_config.hdr_meta.maxcll = maxcll;
                    }
                    PGenSetConfCommand::SetHdrMaxFALL(maxfall) => {
                        pgen_info.output_config.hdr_meta.maxfall = maxfall;
                    }
                }
            }
        }

        if changed_mode {
            self.state.set_pattern_size_and_pos_from_resolution();
        }
    }
}
