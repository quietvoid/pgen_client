use eframe::{
    egui::{Context, ScrollArea, TextureOptions, Ui},
    epaint::{ColorImage, TextureHandle},
};
use kolor_64::ColorConversion;
use serde::{Deserialize, Serialize};

mod cie_diagram_plot;
mod gamma_tracking_plot;
mod luminance_plot;
mod results_summary;
mod rgb_balance_plot;

use cie_diagram_plot::draw_cie_diagram_plot;
use gamma_tracking_plot::draw_gamma_tracking_plot;
use luminance_plot::draw_luminance_plot;
use rgb_balance_plot::draw_rgb_balance_plot;

use crate::{
    calibration::{LuminanceEotf, ReadingResult, TargetColorspace},
    generators::internal::InternalGenerator,
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
    pub show_gamma_plot: bool,
    pub show_luminance_plot: bool,
    pub show_cie_diagram: bool,
    pub show_deviation_percent: bool,
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

        draw_gamma_tracking_plot(ui, &results, &mut app.cal_state);
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

    pub fn target_rgb_to_xyz_conv(&self) -> ColorConversion {
        ColorConversion::new(self.target_csp.to_kolor(), kolor_64::spaces::CIE_XYZ)
    }
}

impl Default for CalibrationState {
    fn default() -> Self {
        Self {
            spotread_started: false,
            spotread_cli_args: vec![("-y".to_owned(), "l".to_owned())],
            spotread_tmp_args: Default::default(),
            target_csp: Default::default(),
            eotf: LuminanceEotf::Gamma22,
            oetf: true,
            internal_gen: Default::default(),
            cie_texture: Default::default(),
            show_rgb_balance_plot: true,
            show_gamma_plot: true,
            show_luminance_plot: true,
            show_cie_diagram: true,
            show_deviation_percent: false,
        }
    }
}
