use std::ops::RangeInclusive;

use kolor_64::Vec3;

use crate::pgen::{pattern_config::PGenPatternConfig, BitDepth};

pub type Rgb = [u16; 3];

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
pub fn rgb_10b_to_8b(depth: u8, rgb: Rgb) -> [u8; 3] {
    if depth > 8 {
        rgb.map(|c| (c / 4) as u8)
    } else {
        rgb.map(|c| c as u8)
    }
}

pub fn scale_pattern_config_rgb_values(
    pattern_config: &mut PGenPatternConfig,
    depth: u8,
    prev_depth: u8,
    limited_range: bool,
    prev_limited_range: bool,
) {
    if depth == prev_depth && limited_range == prev_limited_range {
        return;
    }

    let diff = depth.abs_diff(prev_depth) as f32;
    if prev_depth == 8 {
        // 8 bit to 10 bit
        pattern_config
            .patch_colour
            .iter_mut()
            .chain(pattern_config.background_colour.iter_mut())
            .for_each(|c| {
                *c = scale_8b_rgb_to_10b(*c, diff, depth, limited_range, prev_limited_range)
            });
    } else {
        // 10 bit to 8 bit
        pattern_config
            .patch_colour
            .iter_mut()
            .chain(pattern_config.background_colour.iter_mut())
            .for_each(|c| {
                let mut val = *c as f32 / 2.0_f32.powf(diff);
                val = scale_rgb_into_range(val, depth, limited_range, prev_limited_range);

                *c = val.round() as u16;
            });
    }

    pattern_config.bit_depth = BitDepth::from_repr(depth as usize).unwrap();
    pattern_config.limited_range = limited_range;
}

/// Returns the min as well max - min as the real max value
pub fn get_rgb_real_range(limited_range: bool, bit_depth: u8) -> (u16, u16) {
    let rgb_range = compute_rgb_range(limited_range, bit_depth);
    let min = *rgb_range.start();
    let real_max = *rgb_range.end() - min;

    (min, real_max)
}

pub fn rgb_to_float(rgb: Rgb, limited_range: bool, bit_depth: u8) -> Vec3 {
    let (min, real_max) = get_rgb_real_range(limited_range, bit_depth);
    let real_max = real_max as f64;

    rgb.map(|c| (c - min) as f64 / real_max).into()
}

pub fn round_colour(rgb: Vec3) -> Vec3 {
    (rgb * 1e6).round() / 1e6
}

pub fn normalize_float_rgb_components(rgb: Vec3) -> Vec3 {
    let max = rgb.max_element();
    if max > 0.0 {
        let normalized = (rgb * (1.0 / max)).clamp(Vec3::ZERO, Vec3::ONE);

        round_colour(normalized)
    } else {
        rgb
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        pgen::{pattern_config::PGenPatternConfig, BitDepth},
        utils::scale_pattern_config_rgb_values,
    };

    #[test]
    fn test_8bit_override() {
        let mut new_pattern_cfg = PGenPatternConfig {
            patch_colour: [514, 512, 512],
            background_colour: [64, 64, 64],
            bit_depth: BitDepth::Ten,
            ..Default::default()
        };

        let prev_depth = new_pattern_cfg.bit_depth as u8;
        scale_pattern_config_rgb_values(&mut new_pattern_cfg, 8, prev_depth, false, false);

        assert_eq!(new_pattern_cfg.patch_colour, [129, 128, 128]);
        assert_eq!(new_pattern_cfg.background_colour, [16, 16, 16]);
        assert_eq!(new_pattern_cfg.bit_depth, BitDepth::Eight);
    }
}
