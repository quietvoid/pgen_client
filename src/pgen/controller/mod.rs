use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::Sender, Mutex};

use crate::app::PGenAppUpdate;

pub mod daemon;
pub mod handler;

pub use handler::PGenControllerHandle;

use super::{
    client::{ConnectState, PGenClient},
    commands::{PGenCommand, PGenSetConfCommand},
    pattern_config::PGenPatternConfig,
    BitDepth, ColorFormat, Colorimetry, DoviMapMode, DynamicRange, HdrMetadata, QuantRange,
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
    pub pgen_info: Option<PGenInfo>,
    pub pattern_config: PGenPatternConfig,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct PGenInfo {
    pub version: String,
    pub pid: String,

    pub current_display_mode: DisplayMode,
    pub display_modes: Vec<DisplayMode>,
    pub output_config: PGenOutputConfig,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct PGenOutputConfig {
    pub format: ColorFormat,
    pub bit_depth: BitDepth,
    pub quant_range: QuantRange,
    pub colorimetry: Colorimetry,
    pub dynamic_range: DynamicRange,
    pub hdr_meta: HdrMetadata,

    pub dovi_map_mode: DoviMapMode,
}

#[derive(Debug, Default, Copy, Clone, Deserialize, Serialize, PartialEq)]
pub struct DisplayMode {
    pub id: usize,
    pub resolution: (u16, u16),
    pub refresh_rate: f32,
}

#[derive(Debug, Clone)]
pub enum PGenControllerCmd {
    SetGuiCallback(eframe::egui::Context),
    SetInitialState(PGenControllerState),
    UpdateState(PGenControllerState),
    UpdateSocket(SocketAddr),
    InitialConnect,
    Disconnect,
    TestPattern(PGenPatternConfig),
    SendCurrentPattern,
    SetBlank,
    PGen(PGenCommand),
    RestartSoftware,
    ChangeDisplayMode(DisplayMode),
    MultipleSetConfCommands(Vec<PGenSetConfCommand>),
    UpdateDynamicRange(DynamicRange),
}

impl PGenControllerState {
    pub fn default_socket_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 85)
    }

    pub fn set_pattern_size_and_pos_from_resolution(&mut self) {
        if let Some(pgen_info) = self.pgen_info.as_ref() {
            let (width, height) = pgen_info.current_display_mode.resolution;

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

    pub fn is_dovi_mode(&self) -> bool {
        self.pgen_info
            .as_ref()
            .is_some_and(|e| e.output_config.dynamic_range == DynamicRange::Dovi)
    }
}

impl DisplayMode {
    pub fn try_from_str(line: &str) -> Result<Self> {
        let mut chars = line.chars();

        let id = chars
            .take_while_ref(|c| *c != '[')
            .collect::<String>()
            .parse::<usize>()?;

        // [
        chars.next();

        let resolution = chars
            .take_while_ref(|c| !c.is_whitespace())
            .collect::<String>()
            .split('x')
            .filter_map(|dim| dim.trim_end().parse::<u16>().ok())
            .next_tuple()
            .ok_or_else(|| anyhow!("Failed parsing display resolution"))?;

        // space
        chars.next();

        let refresh_rate = chars
            .take_while(|c| *c != 'H')
            .collect::<String>()
            .trim()
            .parse::<f32>()?;

        Ok(Self {
            id,
            resolution,
            refresh_rate,
        })
    }
}

impl Default for PGenControllerState {
    fn default() -> Self {
        Self {
            socket_addr: PGenControllerState::default_socket_addr(),
            connected_state: Default::default(),
            pgen_info: Default::default(),
            pattern_config: Default::default(),
        }
    }
}

impl std::fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}: {}x{} {}Hz",
            self.id, self.resolution.0, self.resolution.1, self.refresh_rate
        ))
    }
}

#[cfg(test)]
mod test {
    use super::DisplayMode;

    #[test]
    fn parse_display_mode_str() {
        let line = "13[1920x1080 59.94Hz 148.35MHz phsync,pvsync]";

        let mode = DisplayMode::try_from_str(line).unwrap();
        assert_eq!(mode.id, 13);
        assert_eq!(mode.resolution, (1920, 1080));
        assert_eq!(mode.refresh_rate, 59.94);
    }
}
