use kolor_64::Vec3;

/*
 * Port of XYZtoCorColorTemp.c to Rust
 * Author: Bruce Justin Lindbloom
 * Copyright (c) 2003 Bruce Justin Lindbloom. All rights reserved.
*/

/// reciprocal temperature (K)
const RECIPROCAL_TEMP: [f64; 31] = [
    f64::MIN,
    10.0e-6,
    20.0e-6,
    30.0e-6,
    40.0e-6,
    50.0e-6,
    60.0e-6,
    70.0e-6,
    80.0e-6,
    90.0e-6,
    100.0e-6,
    125.0e-6,
    150.0e-6,
    175.0e-6,
    200.0e-6,
    225.0e-6,
    250.0e-6,
    275.0e-6,
    300.0e-6,
    325.0e-6,
    350.0e-6,
    375.0e-6,
    400.0e-6,
    425.0e-6,
    450.0e-6,
    475.0e-6,
    500.0e-6,
    525.0e-6,
    550.0e-6,
    575.0e-6,
    600.0e-6,
];

// UVT LUT
const UVT: [Vec3; 31] = [
    Vec3::new(0.18006, 0.26352, -0.24341),
    Vec3::new(0.18066, 0.26589, -0.25479),
    Vec3::new(0.18133, 0.26846, -0.26876),
    Vec3::new(0.18208, 0.27119, -0.28539),
    Vec3::new(0.18293, 0.27407, -0.30470),
    Vec3::new(0.18388, 0.27709, -0.32675),
    Vec3::new(0.18494, 0.28021, -0.35156),
    Vec3::new(0.18611, 0.28342, -0.37915),
    Vec3::new(0.18740, 0.28668, -0.40955),
    Vec3::new(0.18880, 0.28997, -0.44278),
    Vec3::new(0.19032, 0.29326, -0.47888),
    Vec3::new(0.19462, 0.30141, -0.58204),
    Vec3::new(0.19962, 0.30921, -0.70471),
    Vec3::new(0.20525, 0.31647, -0.84901),
    Vec3::new(0.21142, 0.32312, -1.0182),
    Vec3::new(0.21807, 0.32909, -1.2168),
    Vec3::new(0.22511, 0.33439, -1.4512),
    Vec3::new(0.23247, 0.33904, -1.7298),
    Vec3::new(0.24010, 0.34308, -2.0637),
    Vec3::new(0.24792, 0.34655, -2.4681), /* Note: 0.24792 is a corrected value for the error found in W&S as 0.24702 */
    Vec3::new(0.25591, 0.34951, -2.9641),
    Vec3::new(0.26400, 0.35200, -3.5814),
    Vec3::new(0.27218, 0.35407, -4.3633),
    Vec3::new(0.28039, 0.35577, -5.3762),
    Vec3::new(0.28863, 0.35714, -6.7262),
    Vec3::new(0.29685, 0.35823, -8.5955),
    Vec3::new(0.30505, 0.35907, -11.324),
    Vec3::new(0.31320, 0.35968, -15.628),
    Vec3::new(0.32129, 0.36011, -23.325),
    Vec3::new(0.32931, 0.36038, -40.770),
    Vec3::new(0.33724, 0.36051, -116.45),
];

#[inline(always)]
fn lerp(v: f64, rhs: f64, t: f64) -> f64 {
    v + (rhs - v) * t
}

pub fn xyz_to_cct(xyz: Vec3) -> Option<f64> {
    let mut di = 0.0;

    if (xyz[0] < 1.0e-20) && (xyz[1] < 1.0e-20) && (xyz[2] < 1.0e-20) {
        /* protect against possible divide-by-zero failure */
        return None;
    }

    let us = (4.0 * xyz.x) / (xyz.x + 15.0 * xyz.y + 3.0 * xyz.z);
    let vs = (6.0 * xyz.y) / (xyz.x + 15.0 * xyz.y + 3.0 * xyz.z);
    let mut dm = 0.0;

    let mut i = 0_usize;
    for _ in 0..31 {
        let uvt = UVT[i];
        di = (vs - uvt.y) - uvt.z * (us - uvt.x);

        if (i > 0) && (((di < 0.0) && (dm >= 0.0)) || ((di >= 0.0) && (dm < 0.0))) {
            /* found lines bounding (us, vs) : i-1 and i */
            break;
        }

        dm = di;
        i += 1;
    }

    if i == 31 {
        /* bad XYZ input, color temp would be less than minimum of 1666.7 degrees, or too far towards blue */
        return None;
    }

    di /= (1.0 + UVT[i].z * UVT[i].z).sqrt();
    dm /= (1.0 + UVT[i - 1].z * UVT[i - 1].z).sqrt();

    /* p = interpolation parameter, 0.0 : i-1, 1.0 : i */
    let mut p = dm / (dm - di);
    p = 1.0 / (lerp(RECIPROCAL_TEMP[i - 1], RECIPROCAL_TEMP[i], p));

    Some(p)
}

#[cfg(test)]
mod tests {
    use kolor_64::{
        details::{color::WhitePoint, transform::xyY_to_XYZ},
        Vec3,
    };

    use crate::calibration::cct::xyz_to_cct;

    #[test]
    fn xyz_d65_to_cct() {
        let xyz = xyY_to_XYZ(Vec3::new(0.3127, 0.329, 1.0), WhitePoint::D65);
        let cct = xyz_to_cct(xyz).unwrap();
        assert_eq!(cct, 6503.707184795284);
    }
}
