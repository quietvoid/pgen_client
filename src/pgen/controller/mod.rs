use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::Sender, Mutex};

use crate::app::PGenAppUpdate;

pub mod daemon;
pub mod handler;

pub use handler::PGenControllerHandle;

use super::{
    client::{ConnectState, PGenClient},
    commands::PGenCommand,
    pattern_config::PGenPatternConfig,
    ColorFormat, DynamicRange,
};

#[derive(Debug)]
pub struct PGenControllerContext {
    pub(crate) client: Arc<Mutex<PGenClient>>,

    // For updating the GUI
    pub app_tx: Option<Sender<PGenAppUpdate>>,
    pub(crate) egui_ctx: Option<eframe::egui::Context>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PGenControllerState {
    pub socket_addr: SocketAddr,

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

#[derive(Debug, Clone)]
pub enum PGenControllerCmd {
    SetInitialState(PGenControllerState),
    UpdateState(PGenControllerState),
    UpdateSocket(SocketAddr),
    InitialConnect,
    Disconnect,
    SendCurrentPattern,
    SetBlank,
    PGen(PGenCommand),
    SetGuiCallback(eframe::egui::Context),
    Quit,
}

impl PGenControllerState {
    pub fn default_socket_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 85)
    }

    pub fn set_pattern_size_and_pos_from_resolution(&mut self) {
        if let Some(out_cfg) = &self.output_config {
            let (width, height) = out_cfg.resolution;

            let patch_size = self
                .pattern_config
                .preset_size
                .patch_size_from_display_resolution(width, height);
            let position = self
                .pattern_config
                .preset_position
                .compute_position(width, height, patch_size);

            self.pattern_config.patch_size = patch_size;
            self.pattern_config.position = position;
        }
    }
}

impl Default for PGenControllerState {
    fn default() -> Self {
        Self {
            socket_addr: PGenControllerState::default_socket_addr(),
            connected_state: Default::default(),
            output_config: Default::default(),
            pattern_config: Default::default(),
        }
    }
}
