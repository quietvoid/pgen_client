use eframe::{
    egui::{Context, ScrollArea, TextureOptions, Ui},
    epaint::{ColorImage, TextureHandle},
};
use serde::{Deserialize, Serialize};

mod cie_diagram_plot;
mod luminance_plot;
mod results_summary;
mod rgb_balance_plot;

use cie_diagram_plot::draw_cie_diagram_plot;
use luminance_plot::draw_luminance_plot;
use rgb_balance_plot::draw_rgb_balance_plot;

use crate::{
    calibration::{LuminanceEotf, TargetColorspace},
    generators::internal::InternalGenerator,
    spotread::ReadingResult,
};

pub use cie_diagram_plot::compute_cie_chromaticity_diagram_worker;
use results_summary::draw_results_summary_ui;

use super::PGenApp;

#[derive(Clone, Deserialize, Serialize)]
pub struct CalibrationState {
    pub spotread_started: bool,
    pub spotread_cli_args: Vec<(String, String)>,
    pub spotread_tmp_args: (String, String),

    pub target_csp: TargetColorspace,

    // Luminance calibration
    pub eotf: LuminanceEotf,
    pub oetf: bool,

    pub internal_gen: InternalGenerator,

    #[serde(skip)]
    pub cie_texture: Option<TextureHandle>,

    pub show_rgb_balance_plot: bool,
    pub show_luminance_plot: bool,
    pub show_cie_diagram: bool,
}

pub(crate) fn add_calibration_ui(app: &mut PGenApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        let results = app.cal_state.internal_gen.results();

        if !results.is_empty() {
            draw_results_summary_ui(ui, &mut app.cal_state, &results);
            ui.separator();
        }

        draw_rgb_balance_plot(ui, &mut app.cal_state, &results);
        ui.separator();

        draw_luminance_plot(ui, &results, &mut app.cal_state);
        ui.separator();

        draw_cie_diagram_plot(ui, &mut app.cal_state, &results);
        ui.add_space(10.0);
    });
}

pub(crate) fn handle_spotread_result(app: &mut PGenApp, result: Option<ReadingResult>) {
    log::info!("spotread: {result:?}");

    let internal_gen = &mut app.cal_state.internal_gen;
    if let Some(result) = result {
        if let Some(patch) = internal_gen.selected_patch_mut() {
            patch.result = Some(result);
        }

        let last_idx = internal_gen.list.len() - 1;
        let can_advance =
            internal_gen.auto_advance && internal_gen.selected_idx.is_some_and(|i| i < last_idx);

        let idx = can_advance
            .then_some(internal_gen.selected_idx.as_mut())
            .flatten();
        if let Some(idx) = idx {
            *idx += 1;
        }

        if can_advance {
            app.calibration_send_measure_selected_patch();
        } else {
            internal_gen.started = false;
            app.set_blank();
        }
    } else {
        // Something went wrong and we got no result, stop calibration
        internal_gen.started = false;
        app.set_blank();
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

    pub fn eotf(&self, v: f64) -> f64 {
        match self {
            LuminanceEotf::Gamma22 => v.powf(Self::GAMMA_2_2),
            LuminanceEotf::Gamma24 => v.powf(Self::GAMMA_2_4),
            LuminanceEotf::PQ => Self::pq_to_linear(v),
        }
    }

    pub fn oetf(&self, v: f64) -> f64 {
        match self {
            LuminanceEotf::Gamma22 => v.powf(Self::GAMMA_2_2_INV),
            LuminanceEotf::Gamma24 => v.powf(Self::GAMMA_2_4_INV),
            LuminanceEotf::PQ => Self::linear_to_pq(v),
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

impl CalibrationState {
    pub fn initial_setup(&mut self) {
        self.spotread_started = false;
        self.internal_gen.started = false;
    }

    pub fn set_cie_texture(&mut self, ctx: &Context, image: ColorImage) {
        self.cie_texture.get_or_insert_with(|| {
            ctx.load_texture("cie_xy_diagram_tex", image, TextureOptions::NEAREST)
        });
    }
}

impl Default for CalibrationState {
    fn default() -> Self {
        Self {
            spotread_started: false,
            spotread_cli_args: vec![("-y".to_owned(), "-l".to_owned())],
            spotread_tmp_args: Default::default(),
            target_csp: Default::default(),
            eotf: LuminanceEotf::Gamma22,
            oetf: true,
            internal_gen: Default::default(),
            cie_texture: Default::default(),
            show_rgb_balance_plot: true,
            show_luminance_plot: true,
            show_cie_diagram: true,
        }
    }
}
