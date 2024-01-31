use std::ops::RangeInclusive;

use eframe::{egui::Ui, epaint::Color32};
use egui_plot::{Line, MarkerShape, Plot, PlotPoint, Points};

use super::{CalibrationState, LuminanceEotf, ReadingResult};

pub fn draw_gamma_tracking_plot(
    ui: &mut Ui,
    results: &[ReadingResult],
    cal_state: &mut CalibrationState,
) {
    ui.horizontal(|ui| {
        ui.heading("Gamma");
        ui.checkbox(&mut cal_state.show_gamma_plot, "Show");
    });

    if cal_state.show_gamma_plot {
        draw_plot(ui, results, cal_state.eotf);
    }
}

fn draw_plot(ui: &mut Ui, results: &[ReadingResult], target_eotf: LuminanceEotf) {
    let dark_mode = ui.ctx().style().visuals.dark_mode;
    let ref_color = if dark_mode {
        Color32::from_rgb(0, 255, 255)
    } else {
        Color32::from_rgb(0, 179, 179)
    };
    let lum_color = if dark_mode {
        Color32::YELLOW
    } else {
        Color32::from_rgb(255, 153, 0)
    };

    let minmax_y = ReadingResult::results_minmax_y(results);

    let precision: u32 = 8;
    let max = 2_u32.pow(precision);
    let max_f = max as f64;
    let ref_points: Vec<[f64; 2]> = (0..max)
        .map(|i| {
            let x = i as f64 / max_f;
            let y = target_eotf.gamma_around_zero(x, target_eotf.eotf(x));

            [x, y]
        })
        .collect();

    let ref_line = Line::new(ref_points).color(ref_color).highlight(true);

    let lum_points: Vec<[f64; 2]> = if let Some((min_y, max_y)) = minmax_y {
        results
            .iter()
            .map(|res| {
                let x = res.target.ref_rgb[0];

                let lum = res.luminance(min_y, max_y, target_eotf, false);
                let y = target_eotf.gamma_around_zero(x, lum);

                [x, y]
            })
            .collect()
    } else {
        Vec::new()
    };

    let lum_line = Line::new(lum_points.clone())
        .color(lum_color)
        .highlight(true);
    let lum_markers = Points::new(lum_points)
        .shape(MarkerShape::Circle)
        .radius(2.5)
        .color(lum_color)
        .highlight(true);

    let gamma_mean = target_eotf.mean();
    let gamma_fmt =
        move |tick, _max_digits, _range: &RangeInclusive<f64>| format!("{:.3}", tick + gamma_mean);
    let gamma_label_fmt = move |_s: &str, point: &PlotPoint| {
        format!("x = {:.4}\ny = {:.4}", point.x, point.y + gamma_mean)
    };

    Plot::new("gamma_tracking_plot")
        .view_aspect(2.0)
        .show_background(false)
        .allow_scroll(false)
        .clamp_grid(true)
        .y_axis_formatter(gamma_fmt)
        .label_formatter(gamma_label_fmt)
        .y_grid_spacer(egui_plot::uniform_grid_spacer(move |_| {
            [0.025, 0.075, gamma_mean.round() * 0.1]
        }))
        .show(ui, |plot_ui| {
            plot_ui.line(ref_line);

            plot_ui.line(lum_line);
            plot_ui.points(lum_markers);
        });
}
