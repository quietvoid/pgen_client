use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct PGenPatternConfig {
    pub limited_range: bool,
    pub bit_depth: u8,
    pub patch_colour: [u16; 3],
    pub background_colour: [u16; 3],

    pub position: (u16, u16),
    pub preset_position: TestPatternPosition,
    pub patch_size: (u16, u16),
    pub preset_size: TestPatternSize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TestPatternPosition {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TestPatternSize {
    Percent1,
    Percent2,
    Percent5,
    #[default]
    Percent10,
    Percent25,
    Percent50,
    Percent75,
    Percent100,
}

impl Default for PGenPatternConfig {
    fn default() -> Self {
        Self {
            limited_range: false,
            position: (0, 0),
            patch_size: (0, 0),
            bit_depth: 10,
            patch_colour: [128, 128, 128],
            background_colour: [0, 0, 0],
            preset_position: Default::default(),
            preset_size: Default::default(),
        }
    }
}

impl TestPatternPosition {
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Center => "Center",
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
        }
    }

    pub const fn list() -> &'static [Self] {
        &[
            Self::Center,
            Self::TopLeft,
            Self::TopRight,
            Self::BottomLeft,
            Self::BottomRight,
        ]
    }

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
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Percent1 => "1% window",
            Self::Percent2 => "2% window",
            Self::Percent5 => "5% window",
            Self::Percent10 => "10% window",
            Self::Percent25 => "25% window",
            Self::Percent50 => "50% window",
            Self::Percent75 => "75% window",
            Self::Percent100 => "100% window",
        }
    }

    pub const fn list() -> &'static [Self] {
        &[
            Self::Percent1,
            Self::Percent2,
            Self::Percent5,
            Self::Percent10,
            Self::Percent25,
            Self::Percent50,
            Self::Percent75,
            Self::Percent100,
        ]
    }

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
