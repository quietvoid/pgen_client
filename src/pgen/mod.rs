use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter, FromRepr};

pub mod client;
pub mod commands;
pub mod controller;
pub mod pattern_config;

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum BitDepth {
    #[strum(to_string = "8-bit")]
    Eight = 8,
    #[default]
    #[strum(to_string = "10-bit")]
    Ten = 10,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum DynamicRange {
    #[default]
    #[strum(to_string = "SDR")]
    Sdr,
    #[strum(to_string = "HDR")]
    Hdr,
    #[strum(to_string = "DoVi")]
    Dovi,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum ColorFormat {
    #[default]
    #[strum(to_string = "RGB")]
    Rgb = 0,
    YCbCr444,
    YCbCr422,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum QuantRange {
    Limited = 1,
    #[default]
    Full,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum Colorimetry {
    #[default]
    Default = 0,
    #[strum(to_string = "BT.709 (YCC)")]
    Bt709Ycc = 2,
    #[strum(to_string = "BT.2020 (RGB)")]
    Bt2020Rgb = 9,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct HdrMetadata {
    pub eotf: HdrEotf,
    pub primaries: Primaries,
    pub max_mdl: u16,
    pub min_mdl: u16,
    pub maxcll: u16,
    pub maxfall: u16,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum HdrEotf {
    #[strum(to_string = "Gamma (SDR)")]
    GammaSdr = 0,
    #[strum(to_string = "Gamma (HDR)")]
    GammaHdr = 1,
    #[default]
    #[strum(to_string = "ST.2084 / PQ")]
    Pq = 2,
    #[strum(to_string = "Hybrid log-gamma / HLG")]
    Hlg = 3,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum Primaries {
    #[strum(to_string = "Rec.709")]
    Rec709 = 0,
    #[strum(to_string = "Rec.2020 / D65")]
    Rec2020 = 1,
    #[default]
    #[strum(to_string = "P3 / D65")]
    DisplayP3 = 2,
    #[strum(to_string = "DCI-P3 (Theater)")]
    DciP3 = 3,
    #[strum(to_string = "P3 D60 (ACES Cinema)")]
    P3D60 = 4,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    EnumIter,
    FromRepr,
)]
pub enum DoviMapMode {
    #[strum(to_string = "Verify / Absolute")]
    Absolute = 1,
    #[default]
    #[strum(to_string = "Calibrate / Relative")]
    Relative = 2,
}

impl From<bool> for QuantRange {
    fn from(v: bool) -> Self {
        if v {
            Self::Limited
        } else {
            Self::Full
        }
    }
}
