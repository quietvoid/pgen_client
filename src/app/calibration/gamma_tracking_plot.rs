use std::ops::RangeInclusive;

use eframe::{
    egui::{self, Layout, Ui},
    emath::Align,
    epaint::Color32,
};
use egui_plot::{GridMark, Line, MarkerShape, Plot, PlotPoint, Points};
use strum::IntoEnumIterator;

use super::{CalibrationState, LuminanceEotf, ReadingResult};

pub fn draw_gamma_tracking_plot(
    ui: &mut Ui,
    results: &[ReadingResult],
    cal_state: &mut CalibrationState,
) {
    ui.horizontal(|ui| {
        ui.heading("Gamma");
        ui.checkbox(&mut cal_state.show_gamma_plot, "Show");

        if cal_state.show_gamma_plot {
            let old_eotf = cal_state.eotf;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt(egui::Id::new("cal_luminance_eotf"))
                    .selected_text(cal_state.eotf.as_ref())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);
                        for eotf in LuminanceEotf::iter() {
                            ui.selectable_value(&mut cal_state.eotf, eotf, eotf.as_ref());
                        }
                    });
            });
            if old_eotf != cal_state.eotf {
                cal_state.update_patterns_target();
            }
        }
    });

    if cal_state.show_gamma_plot {
        draw_plot(ui, results, cal_state);
    }
}

fn draw_plot(ui: &mut Ui, results: &[ReadingResult], cal_state: &CalibrationState) {
    let min = cal_state.min_normalized();
    let target_eotf = cal_state.eotf;

    let dark_mode = ui.ctx().style().visuals.dark_mode;
    let ref_pq_color = if dark_mode {
        Color32::GRAY
    } else {
        Color32::DARK_GRAY
    };
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

    let is_pq = target_eotf == LuminanceEotf::PQ;
    let max_pq = is_pq.then(|| target_eotf.oetf(cal_state.max_hdr_mdl / 10_000.0));
    let ref_pq_line = is_pq.then(|| {
        Line::new(vec![[0.0, 0.0], [1.0, 1.0]])
            .color(ref_pq_color)
            .style(egui_plot::LineStyle::Dashed { length: 10.0 })
    });

    let precision: u32 = 8;
    let max = 2_u32.pow(precision);
    let max_f = max as f64;
    let ref_points: Vec<[f64; 2]> = (0..max)
        .filter_map(|i| {
            let x = i as f64 / max_f;
            if x > 0.01 {
                let y = if let Some(max_pq) = max_pq {
                    x.min(max_pq)
                } else {
                    let v_out = target_eotf.value_bpc(min, x, false, false);
                    target_eotf.gamma_around_zero(x, v_out)
                };

                Some([x, y])
            } else {
                None
            }
        })
        .collect();

    let ref_line = Line::new(ref_points).color(ref_color).highlight(true);

    let lum_points: Vec<[f64; 2]> = results
        .iter()
        .filter(|res| res.is_white_stimulus_reading() && res.not_zero_or_one_rgb())
        .map(|res| {
            let x = res.target.ref_rgb[0];
            let y = if is_pq {
                target_eotf.oetf(res.xyz.y / 10_000.0)
            } else {
                res.gamma_around_zero().unwrap()
            };

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

    let mut plot = Plot::new("gamma_tracking_plot")
        .view_aspect(2.0)
        .show_background(false)
        .allow_scroll(false)
        .clamp_grid(true);

    if !is_pq {
        let gamma_mean = target_eotf.mean();
        let gamma_fmt = move |mark: GridMark, _range: &RangeInclusive<f64>| {
            format!("{:.3}", mark.value + gamma_mean)
        };
        let gamma_label_fmt = move |_s: &str, point: &PlotPoint| {
            format!("x = {:.4}\ny = {:.4}", point.x, point.y + gamma_mean)
        };

        plot = plot
            .y_axis_formatter(gamma_fmt)
            .label_formatter(gamma_label_fmt)
            .y_grid_spacer(egui_plot::uniform_grid_spacer(move |_| {
                [0.025, 0.075, gamma_mean.round() * 0.1]
            }));
    }

    plot.show(ui, |plot_ui| {
        if let Some(ref_pq_line) = ref_pq_line {
            plot_ui.line(ref_pq_line);
        }

        plot_ui.line(ref_line);

        plot_ui.line(lum_line);
        plot_ui.points(lum_markers);
    });
}
