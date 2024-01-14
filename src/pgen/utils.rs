use std::ops::RangeInclusive;

pub fn compute_rgb_range(limited_range: bool, depth: u8) -> RangeInclusive<u16> {
    let depth = depth as u32;
    let min_rgb_value = if limited_range {
        16 * 2_u16.pow(depth - 8)
    } else {
        0
    };

    let max_rgb_value = if limited_range {
        let val = if depth == 10 { 219 * 4 } else { 219 };
        val + min_rgb_value
    } else {
        2_u16.pow(depth) - 1
    };

    min_rgb_value..=max_rgb_value
}

pub fn scale_rgb_into_range(
    val: f32,
    depth: u8,
    limited_range: bool,
    prev_limited_range: bool,
) -> f32 {
    if prev_limited_range != limited_range {
        let limited_max = 219.0;
        let (min, max, limited_max) = if depth == 8 {
            (16.0, 255.0, limited_max)
        } else {
            (64.0, 1023.0, limited_max * 4.0)
        };

        if prev_limited_range && !limited_range {
            // Limited to Full
            (val - min) / limited_max * max
        } else {
            // Full to Limited
            ((val / max) * limited_max) + min
        }
    } else {
        val
    }
}

pub fn scale_8b_rgb_to_10b(
    c: u16,
    diff: f32,
    depth: u8,
    limited_range: bool,
    prev_limited_range: bool,
) -> u16 {
    let orig_val = c as f32;
    let mut val = scale_rgb_into_range(orig_val, 8, limited_range, prev_limited_range);

    if depth > 8 {
        // Exception to map old range max to new range
        if u8::MAX as f32 - orig_val <= f32::EPSILON {
            val = 2.0_f32.powf(depth as f32) - 1.0;
        }

        val *= 2.0_f32.powf(diff);
        val.round() as u16
    } else {
        val.round() as u16
    }
}

// Converts for 10 bit otherwise casts to u8
pub fn rgb_10b_to_8b(depth: u8, rgb: [u16; 3]) -> [u8; 3] {
    if depth > 8 {
        rgb.map(|c| (c / 4) as u8)
    } else {
        rgb.map(|c| c as u8)
    }
}
