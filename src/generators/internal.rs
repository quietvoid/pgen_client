use itertools::Itertools;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter};

use crate::{
    calibration::{ReadingResult, RGB_PRIMARIES, RGB_SECONDARIES},
    pgen::pattern_config::PGenPatternConfig,
    utils::{get_rgb_real_range, Rgb},
};

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct InternalGenerator {
    pub started: bool,
    pub auto_advance: bool,
    pub preset: PatchListPreset,

    /// Patch list
    pub list: Vec<InternalPattern>,
    /// Selected patch from list
    pub selected_idx: Option<usize>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct InternalPattern {
    pub rgb: Rgb,

    #[serde(skip)]
    pub result: Option<ReadingResult>,
}

#[derive(
    Display, AsRefStr, Default, Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize, EnumIter,
)]
pub enum PatchListPreset {
    #[default]
    Basic,

    Primaries,
    Secondaries,

    #[strum(to_string = "22 pts greyscale")]
    Greyscale,
    #[strum(to_string = "Saturation sweep")]
    SaturationSweep,
    #[strum(to_string = "Min/max brightness")]
    MinMax,
}

impl InternalGenerator {
    pub fn load_preset(&mut self, config: &PGenPatternConfig) {
        let (min, real_max) = get_rgb_real_range(config.limited_range, config.bit_depth as u8);
        let (min, real_max) = (min as f64, real_max as f64);

        self.list.clear();

        let float_rgb = self.preset.rgb_float_list();
        let scaled_rgb = float_rgb.into_iter().map(|float_rgb| {
            let rgb = float_rgb.map(|c| ((c * real_max) + min).round() as u16);
            InternalPattern {
                rgb,
                ..Default::default()
            }
        });
        self.list.extend(scaled_rgb);
    }

    pub fn selected_patch(&self) -> Option<&InternalPattern> {
        self.selected_idx.and_then(|i| self.list.get(i))
    }

    pub fn selected_patch_mut(&mut self) -> Option<&mut InternalPattern> {
        self.selected_idx.and_then(|i| self.list.get_mut(i))
    }

    pub fn results(&self) -> Vec<ReadingResult> {
        self.list.iter().filter_map(|e| e.result).collect()
    }

    pub fn minmax_y(&self) -> Option<(f64, f64)> {
        self.results()
            .iter()
            .map(|res| res.xyy[2])
            .minmax_by(|a, b| a.total_cmp(b))
            .into_option()
    }
}

impl PatchListPreset {
    pub fn rgb_float_list(&self) -> Vec<[f64; 3]> {
        match self {
            Self::Basic => {
                let mut list = Vec::with_capacity(5);
                list.push([0.0, 0.0, 0.0]);
                list.extend(RGB_PRIMARIES);
                list.push([1.0, 1.0, 1.0]);

                list
            }
            Self::Primaries => RGB_PRIMARIES.to_vec(),
            Self::Secondaries => RGB_SECONDARIES.to_vec(),
            Self::Greyscale => {
                let mut list = Vec::with_capacity(23);
                list.extend(&[
                    [0.0, 0.0, 0.0],
                    [0.025, 0.025, 0.025],
                    [0.05, 0.05, 0.05],
                    [0.075, 0.075, 0.075],
                ]);

                let start = 0.1;
                let step = 0.5;
                let rest = (0..19).map(|i| {
                    let v = ((i as f64 / 10.0) * step) + start;
                    let v = (v * 100.0).round() / 100.0;

                    [v, v, v]
                });
                list.extend(rest);

                list
            }
            Self::SaturationSweep => {
                let mut list = Vec::with_capacity(RGB_SECONDARIES.len() * 4);

                let points = 4;
                let step = 1.0 / points as f32;
                RGB_SECONDARIES.into_iter().for_each(|cmp| {
                    let (h, _, v) = ecolor::hsv_from_rgb(cmp.map(|c| c as f32));

                    // In order of less sat to full sat
                    let sweep = (1..=points).map(|i| {
                        let new_sat = i as f32 * step;
                        ecolor::rgb_from_hsv((h, new_sat, v)).map(|e| e as f64)
                    });
                    list.extend(sweep);
                });

                list
            }
            Self::MinMax => {
                vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]
            }
        }
    }
}
