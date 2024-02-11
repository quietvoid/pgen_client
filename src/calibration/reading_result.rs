use anyhow::{anyhow, bail, Result};
use deltae::{DEMethod::DE2000, Delta, DeltaE};
use itertools::Itertools;
use kolor_64::{
    details::{
        color::WhitePoint,
        transform::{self, XYZ_to_CIELAB, XYZ_to_xyY},
    },
    ColorConversion, Vec3,
};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::utils::round_colour;

use super::{xyz_to_cct, CalibrationTarget, LuminanceEotf, MyLab};

static RESULT_XYZ_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"XYZ:\s(-?\d+\.\d+)\s(-?\d+\.\d+)\s(-?\d+\.\d+)").unwrap());
static RESULT_LAB_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Lab:\s(-?\d+\.\d+)\s(?<a>-?\d+\.\d+)\s(?<b>-?\d+\.\d+)").unwrap());

#[derive(Debug, Default, Clone, Copy)]
pub struct ReadingResult {
    pub target: CalibrationTarget,
    // From sample, ArgyllCMS spotread value
    pub xyz: Vec3,
    pub argyll_lab: Vec3,

    // Calculated from XYZ, D65
    pub xyy: Vec3,
    pub lab: Vec3,
    pub cct: f64,

    // Gamma RGB relative to display peak
    // Calculated from the target primaries
    pub rgb: Vec3,
}

impl ReadingResult {
    pub fn from_spotread_result(target: CalibrationTarget, line: &str) -> Result<Self> {
        let res = RESULT_XYZ_REGEX.captures(line).and_then(|xyz_caps| {
            RESULT_LAB_REGEX
                .captures(line)
                .map(|lab_caps| (xyz_caps, lab_caps))
        });
        if res.is_none() {
            bail!("Failed parsing spotread result: {line}");
        }

        let (xyz_caps, lab_caps) = res.unwrap();

        let (x, y, z) = xyz_caps
            .extract::<3>()
            .1
            .iter()
            .filter_map(|e| e.parse::<f64>().ok())
            .collect_tuple()
            .ok_or_else(|| anyhow!("expected 3 values for XYZ"))?;
        let (l, a, b) = lab_caps
            .extract::<3>()
            .1
            .iter()
            .filter_map(|e| e.parse::<f64>().ok())
            .collect_tuple()
            .ok_or_else(|| anyhow!("expected 3 values for Lab"))?;

        let xyz = Vec3::new(x, y, z);
        let argyll_lab = Vec3::new(l, a, b);

        Ok(Self::from_argyll_results(target, xyz, argyll_lab))
    }

    pub fn from_argyll_results(target: CalibrationTarget, xyz: Vec3, argyll_lab: Vec3) -> Self {
        let mut res = Self {
            target,
            xyz,
            argyll_lab,
            ..Default::default()
        };
        res.set_or_update_calculated_values();

        res
    }

    pub fn set_or_update_calculated_values(&mut self) {
        let xyy = transform::XYZ_to_xyY(self.xyz, WhitePoint::D65);
        self.xyy = round_colour(xyy);

        let lab = transform::XYZ_to_CIELAB(self.xyz / self.target.max_y, WhitePoint::D65);
        self.lab = round_colour(lab);

        self.cct = xyz_to_cct(self.xyz).unwrap_or_default();

        // XYZ -> linear RGB, scaled to display peak
        let dst_csp = self.target.colorspace.to_kolor();
        let rgb_conv = ColorConversion::new(kolor_64::spaces::CIE_XYZ, dst_csp);
        self.rgb = round_colour(rgb_conv.convert(self.xyz));
    }

    pub fn target_min_normalized(&self) -> f64 {
        self.target.min_y / self.target.max_y
    }

    pub fn luminance(&self, oetf: bool) -> f64 {
        let target_eotf = self.target.eotf;
        let (min_y, max_y) = if target_eotf == LuminanceEotf::PQ {
            (0.0, self.target.max_hdr_mdl)
        } else {
            (self.target.min_y, self.target.max_y)
        };

        let y = self.xyy[2] / max_y;

        if oetf {
            target_eotf.oetf(target_eotf.value(y, true))
        } else {
            // Y, minY and maxY are all in display-gamma space
            // And we convert them to linear luminance, so min needs to be decoded to linear
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

    pub fn gamma(&self) -> Option<f64> {
        if self.is_white_stimulus_reading() && self.not_zero_or_one_rgb() {
            let lum = self.luminance(false);

            let ref_stimulus = self.ref_rgb_linear_bpc().x;
            Some(LuminanceEotf::gamma(ref_stimulus, lum))
        } else {
            None
        }
    }

    pub fn gamma_around_zero(&self) -> Option<f64> {
        self.gamma().map(|gamma| gamma - self.target.eotf.mean())
    }

    // BPC applied to target ref RGB in linear space
    pub fn ref_rgb_linear_bpc(&self) -> Vec3 {
        let min = self.target.eotf.oetf(self.target_min_normalized());
        let max = 1.0 - min;

        (self.target.ref_rgb * max) + min
    }

    // Encode linear RGB to target EOTF, need to be relative to the target display
    // The XYZ is scaled to current measured max Y
    pub fn ref_xyz_display_space(
        &self,
        target_rgb_to_xyz: ColorConversion,
        scale_to_y: bool,
    ) -> Vec3 {
        let is_pq = self.target.eotf == LuminanceEotf::PQ;
        let ref_rgb_clipped = if is_pq {
            // Clip to MDL PQ code, since ref display is expected to clip
            let max_pq = self.target.eotf.oetf(self.target.max_hdr_mdl / 10_000.0);
            self.target.ref_rgb.min(Vec3::new(max_pq, max_pq, max_pq))
        } else {
            self.ref_rgb_linear_bpc()
        };

        let mut ref_rgb = self.target.eotf.convert_vec(ref_rgb_clipped, false);

        // To nits
        if is_pq {
            ref_rgb *= 10_000.0;
        }

        let xyz = target_rgb_to_xyz.convert(ref_rgb);
        if !is_pq && scale_to_y {
            xyz * self.target.max_y
        } else {
            xyz
        }
    }

    // The Y is scaled to current measured max Y
    pub fn ref_xyy_display_space(&self, target_rgb_to_xyz: ColorConversion) -> Vec3 {
        let xyz = self.ref_xyz_display_space(target_rgb_to_xyz, true);
        XYZ_to_xyY(xyz, WhitePoint::D65)
    }

    pub fn ref_lab_display_space(&self, target_rgb_to_xyz: ColorConversion) -> Vec3 {
        let ref_xyz = self.ref_xyz_display_space(target_rgb_to_xyz, false);

        // Calculated L*a*b* is in D65
        XYZ_to_CIELAB(ref_xyz, WhitePoint::D65)
    }

    pub fn delta_e2000(&self, target_rgb_to_xyz: ColorConversion) -> DeltaE {
        let mut ref_lab = self.ref_lab_display_space(target_rgb_to_xyz);
        ref_lab.x = self.lab.x;

        MyLab(ref_lab).delta(MyLab(self.lab), DE2000)
    }

    pub fn delta_e2000_incl_luminance(&self, target_rgb_to_xyz: ColorConversion) -> DeltaE {
        let ref_lab = self.ref_lab_display_space(target_rgb_to_xyz);

        MyLab(ref_lab).delta(MyLab(self.lab), DE2000)
    }

    // All equal and not zero, means we're measuring white with stimulus
    pub fn is_white_stimulus_reading(&self) -> bool {
        let ref_red = self.target.ref_rgb.x;
        self.target.ref_rgb.to_array().iter().all(|e| *e == ref_red)
    }

    pub fn not_zero_or_one_rgb(&self) -> bool {
        (0.01..1.0).contains(&self.target.ref_rgb.x)
    }

    pub fn results_average_delta_e2000(
        results: &[Self],
        target_rgb_to_xyz: ColorConversion,
    ) -> f32 {
        let deltae_2000_sum: f32 = results
            .iter()
            .map(|e| *e.delta_e2000(target_rgb_to_xyz).value())
            .sum();

        deltae_2000_sum / results.len() as f32
    }

    pub fn results_average_delta_e2000_incl_luminance(
        results: &[Self],
        target_rgb_to_xyz: ColorConversion,
    ) -> f32 {
        let deltae_2000_sum: f32 = results
            .iter()
            .map(|e| *e.delta_e2000_incl_luminance(target_rgb_to_xyz).value())
            .sum();

        deltae_2000_sum / results.len() as f32
    }

    pub fn results_average_gamma(results: &[Self]) -> Option<f64> {
        let gamma_sum: f64 = results.iter().filter_map(|e| e.gamma()).sum();

        Some(gamma_sum / results.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use kolor_64::{
        details::{color::WhitePoint, transform::XYZ_to_xyY},
        spaces::CIE_XYZ,
        ColorConversion, Vec3,
    };

    use crate::{
        calibration::{LuminanceEotf, TargetColorspace},
        utils::round_colour,
    };

    use super::{CalibrationTarget, ReadingResult};

    #[test]
    fn parse_reading_str() {
        let line =
            "Result is XYZ: 1.916894 2.645760 2.925977, D50 Lab: 18.565392 -13.538479 -6.117640";
        let target = CalibrationTarget::default();

        let reading = ReadingResult::from_spotread_result(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(1.916894, 2.645_76, 2.925977));
        assert_eq!(
            reading.argyll_lab,
            Vec3::new(18.565392, -13.538_479, -6.11764)
        );
    }

    #[test]
    fn parse_white_reference_str() {
        let line = "Making result XYZ: 1.916894 2.645760 2.925977, D50 Lab: 18.565392 -13.538479 -6.117640 white reference.";
        let target = CalibrationTarget::default();

        let reading = ReadingResult::from_spotread_result(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(1.916894, 2.645_76, 2.925977));
        assert_eq!(
            reading.argyll_lab,
            Vec3::new(18.565392, -13.538_479, -6.11764)
        );
    }

    #[test]
    fn calculate_result_rgb() {
        let line = "Result is XYZ: 33.956292 19.408215 138.000457, D50 Lab: 51.161418 63.602645 -121.627088";

        let target = CalibrationTarget {
            ref_rgb: Vec3::new(0.25024438, 0.25024438, 1.0),
            ..Default::default()
        };

        let reading = ReadingResult::from_spotread_result(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(33.956292, 19.408215, 138.000457));
        assert_eq!(
            reading.argyll_lab,
            Vec3::new(51.161418, 63.602645, -121.627088)
        );

        assert_eq!(reading.rgb, Vec3::new(11.403131, 9.232091, 143.827225));
    }

    #[test]
    fn calculate_result_rgb_gray() {
        let line =
            "Result is XYZ: 5.509335 5.835576 5.835576, D50 Lab: 28.993788 -1.357676 -7.541553";

        let target = CalibrationTarget {
            ref_rgb: Vec3::new(0.05, 0.05, 0.05),
            ..Default::default()
        };

        let reading = ReadingResult::from_spotread_result(target, line).unwrap();
        assert_eq!(reading.xyz, Vec3::new(5.509335, 5.835576, 5.835576));
        assert_eq!(
            reading.argyll_lab,
            Vec3::new(28.993788, -1.357676, -7.541553)
        );
        assert_eq!(reading.xyy, Vec3::new(0.320674, 0.339663, 5.835576));
        assert_eq!(reading.lab, Vec3::new(28.993789, -0.434605, 2.169734));

        assert_eq!(reading.rgb, Vec3::new(5.973441, 5.850096, 5.285468));
    }

    #[test]
    fn ref_values_from_rgb() {
        // 5% stimulus
        let target = CalibrationTarget {
            ref_rgb: Vec3::new(0.5, 0.5, 0.5),
            ..Default::default()
        };
        let target_rgb_to_xyz = ColorConversion::new(target.colorspace.to_kolor(), CIE_XYZ);

        let reading = ReadingResult {
            target,
            ..Default::default()
        };
        assert_eq!(reading.ref_rgb_linear_bpc(), Vec3::new(0.5, 0.5, 0.5));

        // EOTF encoded
        assert_eq!(
            reading.ref_xyz_display_space(target_rgb_to_xyz, true),
            Vec3::new(20.685804847401677, 21.763764082403103, 23.697039245842973)
        );
        assert_eq!(
            reading.ref_xyy_display_space(target_rgb_to_xyz),
            Vec3::new(0.31272661468101204, 0.3290231303260619, 21.763764082403103)
        );
        assert_eq!(
            reading.ref_lab_display_space(target_rgb_to_xyz),
            Vec3::new(53.77545209276276, 0.0, 0.0)
        );
    }

    #[test]
    fn ref_values_from_rgb_with_bpc() {
        // 5% stimulus
        let target = CalibrationTarget {
            min_y: 0.1,
            ref_rgb: Vec3::new(0.5, 0.5, 0.5),
            ..Default::default()
        };
        let target_rgb_to_xyz = ColorConversion::new(target.colorspace.to_kolor(), CIE_XYZ);

        let reading = ReadingResult {
            target,
            ..Default::default()
        };

        assert_eq!(
            reading.ref_rgb_linear_bpc(),
            Vec3::new(0.5216438064054153, 0.5216438064054153, 0.5216438064054153)
        );

        // EOTF encoded, scaled to max Y
        assert_eq!(
            reading.ref_xyz_display_space(target_rgb_to_xyz, true),
            Vec3::new(22.707082363307524, 23.890372513922078, 26.01255430433378)
        );
        assert_eq!(
            reading.ref_xyy_display_space(target_rgb_to_xyz),
            Vec3::new(0.3127266146810121, 0.32902313032606184, 23.890372513922078)
        );
        assert_eq!(
            reading.ref_lab_display_space(target_rgb_to_xyz),
            Vec3::new(55.97786542816817, 5.551115123125783e-14, 0.0)
        );
    }

    #[test]
    fn delta_e2000_calc() {
        // 100% stimulus
        let target = CalibrationTarget {
            min_y: 0.13,
            max_y: 130.0,
            ref_rgb: Vec3::new(1.0, 1.0, 1.0),
            ..Default::default()
        };
        let target_rgb_to_xyz = ColorConversion::new(target.colorspace.to_kolor(), CIE_XYZ);

        let xyz = Vec3::new(122.495956, 128.990751, 139.074044);
        let argyll_lab = Vec3::new(110.273101, -2.752364, -20.324487);
        let reading = ReadingResult::from_argyll_results(target, xyz, argyll_lab);

        assert_eq!(0.677219, *reading.delta_e2000(target_rgb_to_xyz).value());
        assert_eq!(
            0.6988424,
            *reading
                .delta_e2000_incl_luminance(target_rgb_to_xyz)
                .value()
        );
    }

    #[test]
    fn test_xyz() {
        let target = CalibrationTarget {
            min_y: 0.132061,
            max_y: 129.072427,
            ref_rgb: Vec3::new(0.500489, 0.500489, 0.500489),
            ..Default::default()
        };

        let xyz = Vec3::new(26.976765, 28.54357, 30.785474);
        let argyll_lab = Vec3::new(60.376676, -2.187671, -12.309911);
        let reading = ReadingResult::from_argyll_results(target, xyz, argyll_lab);

        let target_rgb_to_xyz = ColorConversion::new(target.colorspace.to_kolor(), CIE_XYZ);
        let ref_lab = reading.ref_lab_display_space(target_rgb_to_xyz);

        let de2000 = reading.delta_e2000(target_rgb_to_xyz);
        let de2000_incl_lum = reading.delta_e2000_incl_luminance(target_rgb_to_xyz);

        assert_eq!(ref_lab, Vec3::new(56.048075289473815, 0.0, 0.0));
        assert_eq!(reading.lab, Vec3::new(54.148156, -0.569625, 0.382084));

        assert_eq!(*de2000.value(), 0.91667163);
        assert_eq!(*de2000_incl_lum.value(), 2.0169756);
    }

    #[test]
    fn test_ref_xyz_pq_absolute() {
        let target = CalibrationTarget {
            min_y: 0.0,
            max_y: 800.0,
            max_hdr_mdl: 1000.0,
            ref_rgb: Vec3::new(0.5, 0.5, 0.5),
            colorspace: TargetColorspace::DisplayP3,
            eotf: LuminanceEotf::PQ,
        };

        let xyz = Vec3::new(95.41516, 100.072455, 108.983916);
        let argyll_lab = Vec3::new(100.028009, -1.864209, -19.408698);
        let reading = ReadingResult::from_argyll_results(target, xyz, argyll_lab);

        let target_rgb_to_xyz = ColorConversion::new(target.colorspace.to_kolor(), CIE_XYZ);

        let target_xyz = round_colour(reading.ref_xyz_display_space(target_rgb_to_xyz, true));
        let target_xyy = round_colour(XYZ_to_xyY(target_xyz, WhitePoint::D65));

        assert_eq!(target_xyz, Vec3::new(87.676779, 92.245709, 100.439895));
        assert_eq!(target_xyy, Vec3::new(0.312727, 0.329023, 92.245709));
    }

    #[test]
    fn test_ref_xyz_pq_clip_max() {
        let target = CalibrationTarget {
            min_y: 0.0,
            max_y: 800.0,
            max_hdr_mdl: 800.0,
            ref_rgb: Vec3::new(0.950147, 0.950147, 0.950147),
            colorspace: TargetColorspace::DisplayP3,
            eotf: LuminanceEotf::PQ,
        };

        let xyz = Vec3::new(754.483535, 793.981817, 864.330001);
        let argyll_lab = Vec3::new(215.416777, -4.833889, -38.650394);
        let reading = ReadingResult::from_argyll_results(target, xyz, argyll_lab);

        let target_rgb_to_xyz = ColorConversion::new(target.colorspace.to_kolor(), CIE_XYZ);

        let target_xyz = round_colour(reading.ref_xyz_display_space(target_rgb_to_xyz, true));
        let target_xyy = round_colour(XYZ_to_xyY(target_xyz, WhitePoint::D65));

        assert_eq!(target_xyz, Vec3::new(760.376, 800.0, 871.064));
        assert_eq!(target_xyy, Vec3::new(0.312727, 0.329023, 800.0));
    }
}
