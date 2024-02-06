use deltae::LabValue;
use kolor_64::{ColorSpace, Vec3};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter};

mod cct;
mod luminance_eotf;
mod reading_result;

pub use cct::xyz_to_cct;
pub use luminance_eotf::LuminanceEotf;
pub use reading_result::ReadingResult;

#[derive(Debug, Clone, Copy)]
pub struct CalibrationTarget {
    pub min_y: f64,
    pub max_y: f64,
    pub colorspace: TargetColorspace,
    pub eotf: LuminanceEotf,

    // Linear
    pub ref_rgb: Vec3,
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
    pub const fn to_kolor(&self) -> ColorSpace {
        match self {
            Self::Rec709 => kolor_64::spaces::BT_709,
            Self::DisplayP3 => kolor_64::spaces::DISPLAY_P3,
            Self::Rec2020 => kolor_64::spaces::BT_2020,
        }
    }
}

pub struct MyLab(pub Vec3);
impl From<MyLab> for LabValue {
    fn from(lab: MyLab) -> Self {
        let (l, a, b) = lab.0.to_array().map(|e| e as f32).into();
        LabValue { l, a, b }
    }
}

impl Default for CalibrationTarget {
    fn default() -> Self {
        Self {
            min_y: Default::default(),
            max_y: 100.0,
            colorspace: Default::default(),
            eotf: Default::default(),
            ref_rgb: Default::default(),
        }
    }
}
