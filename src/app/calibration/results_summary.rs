use eframe::egui::{self, Ui};
use itertools::Itertools;

use crate::spotread::ReadingResult;

pub fn draw_results_summary_ui(ui: &mut Ui, results: &[ReadingResult]) {
    let minmax = results
        .iter()
        .map(|res| res.xyy[2] as f64)
        .minmax_by(|a, b| a.total_cmp(b))
        .into_option();

    ui.heading("Results");
    ui.indent("cal_results_summary_indent", |ui| {
        egui::Grid::new("cal_results_summary_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                if let Some((min, max)) = minmax {
                    ui.label(format!("Y Min: {min:.6} nits"));
                    ui.end_row();

                    ui.label(format!("Y Max: {max:.6} nits"));
                    ui.end_row();
                }
            });
    });
}
