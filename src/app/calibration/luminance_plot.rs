use eframe::{
    egui::{self, Layout, Ui},
    emath::Align,
    epaint::Color32,
};
use egui_plot::{Line, MarkerShape, Plot, Points};
use strum::IntoEnumIterator;

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
                egui::ComboBox::from_id_source(egui::Id::new("cal_luminance_eotf"))
                    .selected_text(cal_state.eotf.as_ref())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);
                        for eotf in LuminanceEotf::iter() {
                            ui.selectable_value(&mut cal_state.eotf, eotf, eotf.as_ref());
                        }
                    });
            });
        }
    });

    if cal_state.show_luminance_plot {
        draw_plot(ui, results, cal_state.eotf, cal_state.oetf);
    }
}

fn draw_plot(ui: &mut Ui, results: &[ReadingResult], target_eotf: LuminanceEotf, oetf: bool) {
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

    let minmax_y = ReadingResult::results_minmax_y(results);
    let min_norm = minmax_y
        .and_then(|(min, max)| if max > 0.0 { Some(min / max) } else { None })
        .unwrap_or_default();

    let precision: u32 = 10;
    let max = 2_u32.pow(precision);
    let max_f = max as f64;
    let ref_points: Vec<[f64; 2]> = (0..max)
        .map(|i| {
            let x = i as f64 / max_f;
            [x, target_eotf.value_bpc(min_norm, x, oetf, false)]
        })
        .collect();

    let ref_line = Line::new(ref_points)
        .color(ref_color)
        .highlight(true)
        .style(egui_plot::LineStyle::Dashed { length: 10.0 });

    let lum_points: Vec<[f64; 2]> = if let Some((min_y, max_y)) = minmax_y {
        results
            .iter()
            .map(|res| {
                let x = res.target.ref_rgb[0];
                let y = res.luminance(min_y, max_y, target_eotf, oetf);

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
