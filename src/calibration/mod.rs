use kolor::ColorSpace;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter};

#[derive(
    Display, AsRefStr, Debug, Default, Deserialize, Serialize, Copy, Clone, PartialEq, Eq, EnumIter,
)]
pub enum LuminanceEotf {
    #[default]
    #[strum(to_string = "Gamma 2.2")]
    Gamma22,
    #[strum(to_string = "Gamma 2.4")]
    Gamma24,
    #[strum(to_string = "ST.2084 / PQ")]
    PQ,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReadingTarget {
    pub ref_rgb: [f32; 3],
    pub colorspace: TargetColorspace,
}

#[derive(
    Display, AsRefStr, Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, EnumIter,
)]
pub enum TargetColorspace {
    #[default]
    #[strum(to_string = "Rec. 709")]
    Rec709,
    #[strum(to_string = "Display P3")]
    DisplayP3,
    #[strum(to_string = "Rec. 2020")]
    Rec2020,
}

pub const RGB_PRIMARIES: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
pub const RGB_SECONDARIES: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.0, 1.0, 0.0],
    [1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
];

impl TargetColorspace {
    pub fn to_kolor(&self) -> ColorSpace {
        match self {
            Self::Rec709 => kolor::spaces::BT_709,
            Self::DisplayP3 => kolor::spaces::DISPLAY_P3,
            Self::Rec2020 => kolor::spaces::BT_2020,
        }
    }
}
