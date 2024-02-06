use eframe::{egui::Ui, epaint::Color32};
use egui_plot::{Line, MarkerShape, Plot, Points};
use kolor_64::Vec3;

use crate::calibration::ReadingResult;

use super::CalibrationState;

const RED_MARKER_COLOR: Color32 = Color32::from_rgb(255, 26, 26);
const GREEN_LINE_COLOR: Color32 = Color32::from_rgb(0, 230, 0);
const GREEN_MARKER_COLOR: Color32 = Color32::from_rgb(0, 204, 0);
const BLUE_MARKER_COLOR: Color32 = Color32::from_rgb(51, 51, 255);

pub fn draw_rgb_balance_plot(
    ui: &mut Ui,
    cal_state: &mut CalibrationState,
    results: &[ReadingResult],
) {
    ui.horizontal(|ui| {
        ui.heading("RGB Balance");
        ui.checkbox(&mut cal_state.show_rgb_balance_plot, "Show");
    });

    if cal_state.show_rgb_balance_plot {
        draw_plot(ui, results);
    }
}

fn draw_plot(ui: &mut Ui, results: &[ReadingResult]) {
    let dark_mode = ui.ctx().style().visuals.dark_mode;

    let ref_points: Vec<[f64; 2]> = (0..255).map(|i| [i as f64 / 255.0, 0.0]).collect();
    let ref_color = if dark_mode {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    let ref_line = Line::new(ref_points).color(ref_color);
    let rgb_points: Vec<(f64, Vec3)> = results
        .iter()
        .filter(|res| res.is_white_stimulus_reading() && res.target.ref_rgb.x > 0.01)
        .map(|res| {
            let ref_cmp = res.target.ref_rgb[0];
            let x = (ref_cmp * 1e3).round() / 1e3;

            // Both RGB and min_y are already encoded in display gamma
            let y = res.gamma_normalized_rgb();

            (x, y - 1.0)
        })
        .collect();

    let red_points: Vec<[f64; 2]> = rgb_points.iter().map(|(x, rgb)| [*x, rgb[0]]).collect();
    let red_line = Line::new(red_points.clone())
        .color(Color32::RED)
        .highlight(true);
    let red_markers = Points::new(red_points)
        .shape(MarkerShape::Circle)
        .radius(2.0)
        .color(RED_MARKER_COLOR)
        .highlight(true);

    let green_points: Vec<[f64; 2]> = rgb_points.iter().map(|(x, rgb)| [*x, rgb[1]]).collect();
    let green_line = Line::new(green_points.clone())
        .color(GREEN_LINE_COLOR)
        .highlight(true);
    let green_markers = Points::new(green_points)
        .shape(MarkerShape::Circle)
        .radius(2.0)
        .color(GREEN_MARKER_COLOR)
        .highlight(true);

    let (blue_color, blue_marker_color) = if dark_mode {
        (
            Color32::from_rgb(77, 77, 255),
            Color32::from_rgb(102, 102, 255),
        )
    } else {
        (Color32::BLUE, BLUE_MARKER_COLOR)
    };
    let blue_points: Vec<[f64; 2]> = rgb_points.iter().map(|(x, rgb)| [*x, rgb[2]]).collect();
    let blue_line = Line::new(blue_points.clone())
        .color(blue_color)
        .highlight(true);
    let blue_markers = Points::new(blue_points)
        .shape(MarkerShape::Circle)
        .radius(2.0)
        .color(blue_marker_color)
        .highlight(true);

    Plot::new("rgb_balance_plot")
        .view_aspect(2.0)
        .allow_scroll(false)
        .clamp_grid(true)
        .show_background(false)
        .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [0.025, 0.10, 0.5]))
        .show(ui, |plot_ui| {
            plot_ui.line(ref_line);

            plot_ui.line(red_line);
            plot_ui.points(red_markers);
            plot_ui.line(green_line);
            plot_ui.points(green_markers);
            plot_ui.line(blue_line);
            plot_ui.points(blue_markers);
        });
}
