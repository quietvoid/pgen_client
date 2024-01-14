use serde::{Deserialize, Serialize};

pub mod client;
pub mod commands;
pub mod controller;
pub mod pattern_config;
pub mod utils;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize)]
pub enum DynamicRange {
    #[default]
    Sdr,
    Hdr10,
    LlDv,
    StdDovi,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize)]
pub enum ColorFormat {
    #[default]
    Rgb = 0,
    YCbCr444,
    YCbCr422,
    YCbCr420,
}

impl DynamicRange {
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Sdr => "SDR",
            Self::Hdr10 => "HDR10",
            Self::LlDv => "LLDV",
            Self::StdDovi => "TV-led DoVi",
        }
    }
}

impl ColorFormat {
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Rgb => "RGB",
            Self::YCbCr444 => "YCbCr444",
            Self::YCbCr422 => "YCbCr422",
            Self::YCbCr420 => "YCbCr420",
        }
    }
}
impl From<u8> for ColorFormat {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Rgb,
            1 => Self::YCbCr444,
            2 => Self::YCbCr422,
            3 => Self::YCbCr420,
            _ => unreachable!(),
        }
    }
}
