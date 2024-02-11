use kolor_64::Vec3;
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

    pub fn value_bpc(&self, min: f64, v: f64, oetf: bool, linear_min: bool) -> f64 {
        let min = if *self == Self::PQ {
            0.0
        } else if linear_min {
            min
        } else {
            // Decode min to linear
            self.oetf(min)
        };

        if oetf {
            self.oetf_bpc(min, v)
        } else {
            self.eotf_bpc(min, v)
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

    fn eotf_bpc(&self, min: f64, v: f64) -> f64 {
        let max = 1.0 - min;
        let v = ((v - min) / max).max(0.0);

        (self.eotf(v) * max) + min
    }

    pub fn oetf(&self, v: f64) -> f64 {
        match self {
            Self::Gamma22 => v.powf(Self::GAMMA_2_2_INV),
            Self::Gamma24 => v.powf(Self::GAMMA_2_4_INV),
            Self::PQ => Self::linear_to_pq(v),
        }
    }

    fn oetf_bpc(&self, min: f64, v: f64) -> f64 {
        let max = 1.0 - min;
        (self.oetf(v) * max) + min
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

    pub const fn mean(&self) -> f64 {
        match self {
            Self::Gamma22 => Self::GAMMA_2_2,
            Self::Gamma24 => Self::GAMMA_2_4,
            Self::PQ => 5.0,
        }
    }

    pub fn gamma(v_in: f64, v_out: f64) -> f64 {
        // Avoid division by zero
        let gamma = (v_out.ln() - 1e-7) / (v_in.ln() - 1e-7);
        (gamma * 1e3).round() / 1e3
    }

    pub fn gamma_around_zero(&self, v_in: f64, v_out: f64) -> f64 {
        Self::gamma(v_in, v_out) - self.mean()
    }
}
