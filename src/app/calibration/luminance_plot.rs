use eframe::{
    egui::{Layout, Ui},
    emath::Align,
    epaint::Color32,
};
use egui_plot::{Line, MarkerShape, Plot, Points};

use super::{CalibrationState, LuminanceEotf, ReadingResult};

pub fn draw_luminance_plot(
    ui: &mut Ui,
    results: &[ReadingResult],
    cal_state: &mut CalibrationState,
) {
    ui.horizontal(|ui| {
        ui.heading("Luminance");
        ui.checkbox(&mut cal_state.show_luminance_plot, "Show");

        if cal_state.show_luminance_plot {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.checkbox(&mut cal_state.oetf, "OETF");
            });
        }
    });

    if cal_state.show_luminance_plot {
        let min = cal_state.min_normalized();
        draw_plot(ui, results, min, cal_state);
    }
}

fn draw_plot(ui: &mut Ui, results: &[ReadingResult], min: f64, cal_state: &CalibrationState) {
    let target_eotf = cal_state.eotf;
    let oetf = cal_state.oetf;

    let dark_mode = ui.ctx().style().visuals.dark_mode;
    let ref_color = if dark_mode {
        Color32::GRAY
    } else {
        Color32::DARK_GRAY
    };
    let lum_color = if dark_mode {
        Color32::YELLOW
    } else {
        Color32::from_rgb(255, 153, 0)
    };

    let nits_scale = (target_eotf == LuminanceEotf::PQ).then(|| 10_000.0 / cal_state.max_hdr_mdl);
    let precision: u32 = 8;
    let max = 2_u32.pow(precision);
    let max_f = max as f64;
    let ref_points: Vec<[f64; 2]> = (0..max)
        .map(|i| {
            let fraction = i as f64 / max_f;
            let (x, y) = if let Some(nits_scale) = nits_scale {
                let mut y = target_eotf.value_bpc(0.0, fraction, oetf, false);
                if !oetf {
                    y *= nits_scale;
                }

                (fraction, y.min(1.0))
            } else {
                (fraction, target_eotf.value_bpc(min, fraction, oetf, false))
            };

            [x, y]
        })
        .collect();

    let ref_line = Line::new(ref_points)
        .color(ref_color)
        .highlight(true)
        .style(egui_plot::LineStyle::Dashed { length: 10.0 });

    let lum_points: Vec<[f64; 2]> = results
        .iter()
        .filter(|res| res.is_white_stimulus_reading())
        .map(|res| {
            let x = res.target.ref_rgb[0];
            let y = res.luminance(oetf);

            [x, y]
        })
        .collect();

    let lum_line = Line::new(lum_points.clone())
        .color(lum_color)
        .highlight(true);
    let lum_markers = Points::new(lum_points)
        .shape(MarkerShape::Circle)
        .radius(2.5)
        .color(lum_color)
        .highlight(true);

    Plot::new("luminance_plot")
        .view_aspect(2.0)
        .allow_scroll(false)
        .clamp_grid(true)
        .show_background(false)
        .show(ui, |plot_ui| {
            plot_ui.line(ref_line);

            plot_ui.line(lum_line);
            plot_ui.points(lum_markers);
        });
}
