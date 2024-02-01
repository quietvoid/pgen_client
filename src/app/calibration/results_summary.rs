use eframe::egui::{self, Ui};

use crate::calibration::ReadingResult;

use super::CalibrationState;

pub fn draw_results_summary_ui(
    ui: &mut Ui,
    cal_state: &mut CalibrationState,
    results: &[ReadingResult],
) {
    let target_rgb_to_xyz = cal_state.target_rgb_to_xyz_conv();
    let target_eotf = cal_state.eotf;

    let minmax_y = ReadingResult::results_minmax_y(results);
    let avg_delta_e2000 = ReadingResult::results_average_delta_e2000(
        results,
        minmax_y,
        target_rgb_to_xyz,
        target_eotf,
    );
    let avg_gamma_str = if let Some(avg_gamma) =
        ReadingResult::results_average_gamma(results, minmax_y, target_eotf)
    {
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
                    ui.label(format!("Y Max: {max_y:.6} nits"));
                    ui.end_row();

                    ui.label(format!("Average dE2000: {avg_delta_e2000:.4}"));
                    ui.end_row();

                    ui.label(format!("Average gamma: {avg_gamma_str}"));
                    ui.end_row();
                }
            });
    });
}
