use kolor_64::{ColorSpace, Vec3};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter};

mod cct;
mod luminance_eotf;
mod reading_result;

pub use cct::xyz_to_cct;
pub use luminance_eotf::LuminanceEotf;
pub use reading_result::ReadingResult;

#[derive(Debug, Default, Clone, Copy)]
pub struct ReadingTarget {
    // Linear
    pub ref_rgb: Vec3,
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

pub const RGB_PRIMARIES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
pub const RGB_SECONDARIES: [[f64; 3]; 6] = [
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
            Self::Rec709 => kolor_64::spaces::BT_709,
            Self::DisplayP3 => kolor_64::spaces::DISPLAY_P3,
            Self::Rec2020 => kolor_64::spaces::BT_2020,
        }
    }
}
