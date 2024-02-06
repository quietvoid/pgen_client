use eframe::egui::{self, Ui};

use crate::calibration::ReadingResult;

use super::CalibrationState;

pub fn draw_results_summary_ui(
    ui: &mut Ui,
    cal_state: &mut CalibrationState,
    results: &[ReadingResult],
) {
    let target_rgb_to_xyz = cal_state.target_rgb_to_xyz_conv();

    let minmax_y = cal_state.internal_gen.minmax_y();
    let avg_delta_e2000 = ReadingResult::results_average_delta_e2000(results, target_rgb_to_xyz);
    let avg_delta_e2000_incl_lum =
        ReadingResult::results_average_delta_e2000_incl_luminance(results, target_rgb_to_xyz);
    let avg_gamma_str = if let Some(avg_gamma) = ReadingResult::results_average_gamma(results) {
        format!("{avg_gamma:.4}")
    } else {
        "N/A".to_string()
    };

    ui.heading("Results");
    ui.indent("cal_results_summary_indent", |ui| {
        if ui.button("Clear results").clicked() {
            cal_state.internal_gen.selected_idx = None;
            cal_state.internal_gen.list.iter_mut().for_each(|e| {
                e.result.take();
            })
        }
        egui::Grid::new("cal_results_summary_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                if let Some((min_y, max_y)) = minmax_y {
                    ui.label(format!("Y Min: {min_y:.6} nits"));

                    ui.add_space(5.0);
                    ui.label(format!("Y Max: {max_y:.6} nits"));
                    ui.end_row();
                }

                ui.label(format!("Average dE2000: {avg_delta_e2000:.4}"));
                ui.label(format!(
                    "Average dE2000 w/ lum: {avg_delta_e2000_incl_lum:.4}"
                ));
                ui.end_row();

                ui.label(format!("Average gamma: {avg_gamma_str}"));
                ui.end_row();
            });
    });
}
