use kolor_64::{ColorSpace, Vec3};
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

impl LuminanceEotf {
    const GAMMA_2_2: f64 = 2.2;
    const GAMMA_2_2_INV: f64 = 1.0 / Self::GAMMA_2_2;
    const GAMMA_2_4: f64 = 2.4;
    const GAMMA_2_4_INV: f64 = 1.0 / Self::GAMMA_2_4;

    pub fn value(&self, v: f64, oetf: bool) -> f64 {
        if oetf {
            self.oetf(v)
        } else {
            self.eotf(v)
        }
    }

    pub fn convert_vec(&self, v: Vec3, oetf: bool) -> Vec3 {
        if oetf {
            match self {
                Self::Gamma22 => v.powf(Self::GAMMA_2_2_INV),
                Self::Gamma24 => v.powf(Self::GAMMA_2_4_INV),
                Self::PQ => v.to_array().map(Self::linear_to_pq).into(),
            }
        } else {
            match self {
                Self::Gamma22 => v.powf(Self::GAMMA_2_2),
                Self::Gamma24 => v.powf(Self::GAMMA_2_4),
                Self::PQ => v.to_array().map(Self::pq_to_linear).into(),
            }
        }
    }

    pub fn eotf(&self, v: f64) -> f64 {
        match self {
            Self::Gamma22 => v.powf(Self::GAMMA_2_2),
            Self::Gamma24 => v.powf(Self::GAMMA_2_4),
            Self::PQ => Self::pq_to_linear(v),
        }
    }

    pub fn oetf(&self, v: f64) -> f64 {
        match self {
            Self::Gamma22 => v.powf(Self::GAMMA_2_2_INV),
            Self::Gamma24 => v.powf(Self::GAMMA_2_4_INV),
            Self::PQ => Self::linear_to_pq(v),
        }
    }

    const ST2084_M1: f64 = 2610.0 / 16384.0;
    const ST2084_M2: f64 = (2523.0 / 4096.0) * 128.0;
    const ST2084_C1: f64 = 3424.0 / 4096.0;
    const ST2084_C2: f64 = (2413.0 / 4096.0) * 32.0;
    const ST2084_C3: f64 = (2392.0 / 4096.0) * 32.0;
    fn pq_to_linear(x: f64) -> f64 {
        if x > 0.0 {
            let xpow = x.powf(1.0 / Self::ST2084_M2);
            let num = (xpow - Self::ST2084_C1).max(0.0);
            let den = (Self::ST2084_C2 - Self::ST2084_C3 * xpow).max(f64::NEG_INFINITY);

            (num / den).powf(1.0 / Self::ST2084_M1)
        } else {
            0.0
        }
    }

    fn linear_to_pq(v: f64) -> f64 {
        let num = Self::ST2084_C1 + Self::ST2084_C2 * v.powf(Self::ST2084_M1);
        let denom = 1.0 + Self::ST2084_C3 * v.powf(Self::ST2084_M1);

        (num / denom).powf(Self::ST2084_M2)
    }
}
