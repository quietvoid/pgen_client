use anyhow::{anyhow, Result};
use deltae::{DEMethod::DE2000, Delta, DeltaE};
use itertools::Itertools;
use kolor_64::{
    details::{
        color::WhitePoint,
        transform::{self, XYZ_to_CIELAB, XYZ_to_xyY},
    },
    ColorConversion, Vec3,
};

use crate::utils::round_colour;

use super::{xyz_to_cct, LuminanceEotf, MyLab, ReadingTarget};

#[derive(Debug, Clone, Copy)]
pub struct ReadingResult {
    pub target: ReadingTarget,
    pub xyz: Vec3,
    pub lab: Vec3,

    // xyY from reading XYZ
    pub xyy: Vec3,
    pub cct: f64,

    // Gamma RGB relative to display peak
    // Calculated from the target primaries
    pub rgb: Vec3,
}

impl ReadingResult {
    pub fn new(target: ReadingTarget, line: &str) -> Result<Self> {
        let mut split = line.split(", ");

        let xyz_str = split
            .next()
            .and_then(|e| e.strip_prefix("Result is XYZ: "))
            .ok_or_else(|| anyhow!("expected both XYZ and Lab results"))?;
        let lab_str = split
            .next()
            .and_then(|e| e.strip_prefix("D50 Lab: "))
            .ok_or_else(|| anyhow!("expected Lab results"))?;

        let (x, y, z) = xyz_str
            .split_whitespace()
            .filter_map(|e| e.parse::<f64>().ok())
            .collect_tuple()
            .ok_or_else(|| anyhow!("expected 3 values for XYZ"))?;
        let (l, a, b) = lab_str
            .split_whitespace()
            .filter_map(|e| e.parse::<f64>().ok())
            .collect_tuple()
            .ok_or_else(|| anyhow!("expected 3 values for Lab"))?;

        let xyz = Vec3::new(x, y, z);
        let lab = Vec3::new(l, a, b);

        // XYZ -> linear RGB, scaled to display peak
        let dst_csp = target.colorspace.to_kolor();
        let rgb_conv = ColorConversion::new(kolor_64::spaces::CIE_XYZ, dst_csp);
        let rgb = round_colour(rgb_conv.convert(xyz));

        let xyy = transform::XYZ_to_xyY(xyz, WhitePoint::D65);
        let xyy = round_colour(xyy);
        let cct = xyz_to_cct(xyz).unwrap_or_default();

        Ok(Self {
            target,
            xyz,
            lab,
            xyy,
            cct,
            rgb,
        })
    }

    pub fn luminance(&self, min_y: f64, max_y: f64, target_eotf: LuminanceEotf, oetf: bool) -> f64 {
        let y = self.xyy[2] / max_y;

        // Y, minY and maxY are all in display-gamma space
        // And we convert them to linear luminance, so min needs to be decoded to linear
        if oetf {
            target_eotf.oetf(target_eotf.value(y, true))
        } else {
            let min = target_eotf.oetf(min_y / max_y);
            let max = 1.0 - min;
            (y * max) + min
        }
    }

    pub fn gamma_normalized_rgb(&self) -> Vec3 {
        let actual_rgb = self.rgb;
        let sample_y = self.xyy[2];

        if sample_y > 0.0 {
            actual_rgb / sample_y
        } else {
            actual_rgb
        }
    }

    pub fn gamma(&self, minmax_y: Option<(f64, f64)>, target_eotf: LuminanceEotf) -> Option<f64> {
        if let Some((min_y, max_y)) = minmax_y {
            let ref_stimulus = self.ref_rgb_linear_bpc(minmax_y, target_eotf).x;
            let lum = self.luminance(min_y, max_y, target_eotf, false);
            Some(LuminanceEotf::gamma(ref_stimulus, lum))
        } else {
            None
        }
    }

    pub fn gamma_around_zero(
        &self,
        minmax_y: Option<(f64, f64)>,
        target_eotf: LuminanceEotf,
    ) -> Option<f64> {
        self.gamma(minmax_y, target_eotf)
            .map(|gamma| gamma - target_eotf.mean())
    }

    // BPC applied to target ref RGB in linear space
    pub fn ref_rgb_linear_bpc(
        &self,
        minmax_y: Option<(f64, f64)>,
        target_eotf: LuminanceEotf,
    ) -> Vec3 {
        let ref_rgb = self.target.ref_rgb;

        if let Some((min_y, max_y)) = minmax_y {
            let min = target_eotf.oetf(min_y / max_y);
            let max = 1.0 - min;

            (ref_rgb * max) + min
        } else {
            ref_rgb
        }
    }

    // Encode linear RGB to target EOTF, need to be relative to the target display
    pub fn ref_xyz(
        &self,
        minmax_y: Option<(f64, f64)>,
        target_rgb_to_xyz: ColorConversion,
        target_eotf: LuminanceEotf,
    ) -> Vec3 {
        let xyz = target_rgb_to_xyz.convert(target_eotf.convert_vec(self.target.ref_rgb, false));

        if let Some((min_y, max_y)) = minmax_y {
            // BPC in 0-1
            let min = min_y / max_y;
            let max = 1.0 - min;
            let v = (xyz * max) + min;

            // Scale Y to measured peak
            v * max_y
        } else {
            xyz
        }
    }

    pub fn ref_xyy(
        &self,
        minmax_y: Option<(f64, f64)>,
        target_rgb_to_xyz: ColorConversion,
        target_eotf: LuminanceEotf,
    ) -> Vec3 {
        let xyz = self.ref_xyz(minmax_y, target_rgb_to_xyz, target_eotf);
        XYZ_to_xyY(xyz, WhitePoint::D65)
    }

    pub fn ref_lab(
        &self,
        minmax_y: Option<(f64, f64)>,
        target_rgb_to_xyz: ColorConversion,
        target_eotf: LuminanceEotf,
    ) -> Vec3 {
        let ref_xyz = self.ref_xyz(minmax_y, target_rgb_to_xyz, target_eotf);

        let ref_xyz = if let Some(max_y) = minmax_y.map(|e| e.1) {
            ref_xyz / max_y
        } else {
            ref_xyz
        };

        XYZ_to_CIELAB(ref_xyz, WhitePoint::D65)
    }

    pub fn delta_e2000(
        &self,
        minmax_y: Option<(f64, f64)>,
        target_rgb_to_xyz: ColorConversion,
        target_eotf: LuminanceEotf,
    ) -> DeltaE {
        let ref_lab = self.ref_lab(minmax_y, target_rgb_to_xyz, target_eotf);

        MyLab(ref_lab).delta(MyLab(self.lab), DE2000)
    }

    // All equal and not zero, means we're measuring white with stimulus
    pub fn is_white_stimulus_reading(&self) -> bool {
        let ref_red = self.target.ref_rgb.x;

        if ref_red > 0.0 {
            self.target.ref_rgb.to_array().iter().all(|e| *e == ref_red)
        } else {
            false
        }
    }

    pub fn results_minmax_y(results: &[Self]) -> Option<(f64, f64)> {
        if results.iter().any(|e| e.is_white_stimulus_reading()) {
            results
                .iter()
                .map(|res| res.xyy[2])
                .minmax_by(|a, b| a.total_cmp(b))
                .into_option()
        } else {
            None
        }
    }

    pub fn results_average_delta_e2000(
        results: &[Self],
        minmax_y: Option<(f64, f64)>,
        target_rgb_to_xyz: ColorConversion,
        target_eotf: LuminanceEotf,
    ) -> f32 {
        let deltae_2000_sum: f32 = results
            .iter()
            .map(|e| {
                *e.delta_e2000(minmax_y, target_rgb_to_xyz, target_eotf)
                    .value()
            })
            .sum();

        deltae_2000_sum / results.len() as f32
    }

    pub fn results_average_gamma(
        results: &[Self],
        minmax_y: Option<(f64, f64)>,
        target_eotf: LuminanceEotf,
    ) -> Option<f64> {
        if minmax_y.is_some() {
            let gamma_sum: f64 = results
                .iter()
                .filter_map(|e| e.gamma(minmax_y, target_eotf))
                .sum();

            Some(gamma_sum / results.len() as f64)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use kolor_64::Vec3;

    use super::{ReadingResult, ReadingTarget};

    #[test]
    fn parse_reading_str() {
        let line =
            "Result is XYZ: 1.916894 2.645760 2.925977, D50 Lab: 18.565392 -13.538479 -6.117640";
        let target = ReadingTarget::default();

        let reading = ReadingResult::new(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(1.916894, 2.645_76, 2.925977));
        assert_eq!(reading.lab, Vec3::new(18.565392, -13.538_479, -6.11764));
    }

    #[test]
    fn calculate_result_rgb() {
        let line = "Result is XYZ: 33.956292 19.408215 138.000457, D50 Lab: 51.161418 63.602645 -121.627088";

        let target = ReadingTarget {
            ref_rgb: Vec3::new(0.25024438, 0.25024438, 1.0),
            ..Default::default()
        };

        let reading = ReadingResult::new(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(33.956292, 19.408215, 138.000457));
        assert_eq!(reading.lab, Vec3::new(51.161418, 63.602645, -121.627088));

        assert_eq!(reading.rgb, Vec3::new(11.403131, 9.232091, 143.827225));
    }

    #[test]
    fn calculate_result_rgb_gray() {
        let line =
            "Result is XYZ: 5.509335 5.835576 5.835576, D50 Lab: 28.993788 -1.357676 -7.541553";

        let target = ReadingTarget {
            ref_rgb: Vec3::new(0.029116, 0.029116, 0.029116),
            ..Default::default()
        };

        let reading = ReadingResult::new(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(5.509335, 5.835576, 5.835576));
        assert_eq!(reading.lab, Vec3::new(28.993788, -1.357676, -7.541553));
        assert_eq!(reading.xyy, Vec3::new(0.320674, 0.339663, 5.835576));

        assert_eq!(reading.rgb, Vec3::new(5.973441, 5.850096, 5.285468));
    }
}
