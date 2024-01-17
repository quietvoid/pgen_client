use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter};

use super::BitDepth;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct PGenPatternConfig {
    pub limited_range: bool,
    pub bit_depth: BitDepth,
    pub patch_colour: [u16; 3],
    pub background_colour: [u16; 3],

    pub position: (u16, u16),
    pub preset_position: TestPatternPosition,
    pub patch_size: (u16, u16),
    pub preset_size: TestPatternSize,
}

#[derive(
    Display, AsRefStr, Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, EnumIter,
)]
pub enum TestPatternPosition {
    #[default]
    Center,
    #[strum(to_string = "Top left")]
    TopLeft,
    #[strum(to_string = "Top right")]
    TopRight,
    #[strum(to_string = "Bottom left")]
    BottomLeft,
    #[strum(to_string = "Bottom right")]
    BottomRight,
}

#[derive(
    Display, AsRefStr, Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, EnumIter,
)]
pub enum TestPatternSize {
    #[strum(to_string = "1% window")]
    Percent1,
    #[strum(to_string = "2% window")]
    Percent2,
    #[strum(to_string = "5% window")]
    Percent5,
    #[default]
    #[strum(to_string = "10% window")]
    Percent10,
    #[strum(to_string = "25% window")]
    Percent25,
    #[strum(to_string = "50% window")]
    Percent50,
    #[strum(to_string = "75% window")]
    Percent75,
    #[strum(to_string = "100% window")]
    Percent100,
}

impl Default for PGenPatternConfig {
    fn default() -> Self {
        Self {
            limited_range: false,
            position: (0, 0),
            patch_size: (0, 0),
            bit_depth: BitDepth::Ten,
            patch_colour: [128, 128, 128],
            background_colour: [0, 0, 0],
            preset_position: Default::default(),
            preset_size: Default::default(),
        }
    }
}

impl TestPatternPosition {
    pub fn compute_position(&self, width: u16, height: u16, patch_size: (u16, u16)) -> (u16, u16) {
        match self {
            Self::Center => {
                let (w2, h2) = (width / 2, height / 2);
                let (pw2, ph2) = (patch_size.0 / 2, patch_size.1 / 2);

                (w2 - pw2, h2 - ph2)
            }
            Self::TopLeft => (0, 0),
            Self::TopRight => (width - patch_size.0, 0),
            Self::BottomLeft => (0, height - patch_size.1),
            Self::BottomRight => (width - patch_size.0, height - patch_size.1),
        }
    }
}

impl TestPatternSize {
    pub const fn float(&self) -> f32 {
        match self {
            Self::Percent1 => 0.01,
            Self::Percent2 => 0.02,
            Self::Percent5 => 0.05,
            Self::Percent10 => 0.1,
            Self::Percent25 => 0.25,
            Self::Percent50 => 0.5,
            Self::Percent75 => 0.75,
            Self::Percent100 => 1.0,
        }
    }

    pub fn patch_size_from_display_resolution(&self, width: u16, height: u16) -> (u16, u16) {
        let (width, height) = (width as f32, height as f32);
        let area = width * height;

        let patch_area = self.float() * area;
        let scale = (patch_area / area).sqrt();

        (
            (scale * width).round() as u16,
            (scale * height).round() as u16,
        )
    }
}
