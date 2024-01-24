use std::sync::OnceLock;

use ecolor::{gamma_u8_from_linear_f32, Color32};
use eframe::{
    egui::{Spinner, Ui},
    epaint::{ColorImage, Pos2, Rect, Stroke, Vec2},
};
use egui_plot::{MarkerShape, Plot, PlotImage, PlotPoint, PlotPoints, Points, Polygon};
use itertools::Itertools;
use kolor::{
    details::{color::WhitePoint, transform::xyY_to_XYZ},
    spaces::CIE_XYZ,
    ColorConversion,
};
use ndarray::{
    parallel::prelude::{IntoParallelRefIterator, ParallelIterator},
    Array,
};
use tokio::sync::mpsc::Sender;

use crate::{app::PGenAppUpdate, spotread::ReadingResult};

use super::CalibrationState;

const CIE_1931_2DEG_OBSERVER_DATASET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/CIE_cc_1931_2deg.csv"
));
static CIE_1931_DIAGRAM_POINTS: OnceLock<Vec<SpectralLocusPoint>> = OnceLock::new();

// Calculated from locis coordinates
const XY_TOP_LEFT: Vec2 = Vec2::new(0.00364, 0.83409);
const XY_BOTTOM_RIGHT: Vec2 = Vec2::new(0.73469, 0.00477);

#[derive(Debug, Clone, Copy)]
pub struct SpectralLocusPoint {
    _wavelength: u16,
    x: f32,
    y: f32,
}

pub fn draw_cie_diagram_plot(
    ui: &mut Ui,
    cal_state: &mut CalibrationState,
    results: &[ReadingResult],
) {
    ui.horizontal(|ui| {
        ui.heading("Chromaticity xy");
        ui.checkbox(&mut cal_state.show_cie_diagram, "Show");
    });

    if cal_state.show_cie_diagram {
        draw_diagram(ui, cal_state, results);
    }
}

fn draw_diagram(ui: &mut Ui, cal_state: &mut CalibrationState, results: &[ReadingResult]) {
    if let (Some(texture), Some(locis_points)) = (
        cal_state.cie_texture.as_ref(),
        CIE_1931_DIAGRAM_POINTS.get(),
    ) {
        let dark_mode = ui.ctx().style().visuals.dark_mode;
        let locis_points: Vec<_> = locis_points
            .iter()
            .map(|e| [e.x as f64, e.y as f64])
            .collect();

        let curve_stroke_colour = if dark_mode {
            Color32::from_rgba_unmultiplied(255, 255, 255, 64)
        } else {
            Color32::from_rgba_unmultiplied(96, 96, 96, 96)
        };
        let curve_poly = Polygon::new(PlotPoints::new(locis_points))
            .fill_color(Color32::TRANSPARENT)
            .stroke(Stroke::new(4.0, curve_stroke_colour));

        let img_size = Vec2::new(XY_BOTTOM_RIGHT.x, XY_TOP_LEFT.y);
        let img_center = img_size / 2.0;
        let center_point = PlotPoint::new(img_center.x, img_center.y);
        let image = PlotImage::new(texture.id(), center_point, img_size).uv(Rect::from_two_pos(
            Pos2::new(0.0, 1.0 - XY_TOP_LEFT.y),
            Pos2::new(XY_BOTTOM_RIGHT.x, 1.0),
        ));

        let triangle_colour = if dark_mode {
            Color32::WHITE
        } else {
            Color32::GRAY
        };
        let target_csp = cal_state.target_csp.to_kolor();
        let target_primaries = target_csp
            .primaries()
            .values()
            .map(|xy| xy.map(|c| c as f64))
            .to_vec();
        let target_gamut_triangle = Polygon::new(target_primaries)
            .stroke(Stroke::new(2.0, triangle_colour))
            .fill_color(Color32::TRANSPARENT);

        // Secondaries targets
        let target_rgb_to_xyy =
            kolor::ColorConversion::new(target_csp, kolor::spaces::CIE_XYZ.to_cie_xyY());

        let results_points = results
            .iter()
            .map(|res| Points::new([res.xyy[0] as f64, res.xyy[1] as f64]));
        let results_targets = results
            .iter()
            .map(|res| create_polygon_for_target_rgb(target_rgb_to_xyy, res.target.ref_rgb));

        let target_box_colour = if dark_mode {
            Color32::GRAY
        } else {
            Color32::WHITE
        };
        Plot::new("cie_diagram_plot")
            .data_aspect(1.0)
            .view_aspect(1.5)
            .allow_scroll(false)
            .show_grid(false)
            .clamp_grid(true)
            .show_background(false)
            .show(ui, |plot_ui| {
                plot_ui.image(image);
                plot_ui.polygon(curve_poly);
                plot_ui.polygon(target_gamut_triangle);

                for (center, xy_target) in results_targets {
                    let poly = xy_target
                        .stroke(Stroke::new(2.0, target_box_colour))
                        .fill_color(Color32::TRANSPARENT);
                    let center_dot = Points::new(center)
                        .radius(5.0)
                        .color(Color32::BLACK)
                        .shape(MarkerShape::Cross);

                    plot_ui.polygon(poly);
                    plot_ui.points(center_dot);
                }

                for res_point in results_points {
                    let point = res_point.radius(2.0).color(Color32::RED);
                    plot_ui.points(point);
                }
            });
    } else {
        ui.add(Spinner::new().size(100.0));
    }
}

pub fn compute_cie_chromaticity_diagram_worker(app_tx: Sender<PGenAppUpdate>) {
    tokio::task::spawn(async move {
        let locis = CIE_1931_DIAGRAM_POINTS.get_or_init(|| {
            CIE_1931_2DEG_OBSERVER_DATASET
                .lines()
                .map(|line| {
                    let mut split = line.split(',');

                    let wavelength = split.next().and_then(|e| e.parse().ok()).unwrap();
                    let x = split.next().and_then(|e| e.parse().ok()).unwrap();
                    let y = split.next().and_then(|e| e.parse().ok()).unwrap();

                    SpectralLocusPoint {
                        _wavelength: wavelength,
                        x,
                        y,
                    }
                })
                .collect()
        });
        let locis_points: Vec<_> = locis.iter().map(|locus| [locus.x, locus.y]).collect();

        let img = compute_cie_xy_diagram_image(&locis_points);
        app_tx.try_send(PGenAppUpdate::CieDiagramReady(img)).ok();
    });
}

fn compute_cie_xy_diagram_image(points: &[[f32; 2]]) -> ColorImage {
    let resolution = 4096;

    let x_points = Array::linspace(0.0, 1.0, resolution);
    let y_points = Array::linspace(1.0, 0.0, resolution);
    let grid_points = Array::from_iter(y_points.iter().cartesian_product(x_points.iter()));

    let xyz_conv = kolor::ColorConversion::new(CIE_XYZ, kolor::spaces::BT_709);
    let wp = WhitePoint::D65;

    let pixels: Vec<Color32> = grid_points
        .par_iter()
        .copied()
        .map(|(y, x)| {
            if !point_in_or_on_convex_polygon(points, *x, *y) {
                return Color32::TRANSPARENT;
            }

            let xyy = [*x, *y, 1.0].into();
            let xyz = xyY_to_XYZ(xyy, wp);

            let rgb = xyz_conv.convert(xyz);
            let mut rgb = rgb.to_array();
            let max = rgb.into_iter().max_by(|a, b| a.total_cmp(b));
            if max.is_some_and(|e| e > 0.0) {
                let max = max.unwrap();
                rgb = rgb.map(|c| (c * (1.0 / max)).clamp(0.0, 1.0));
            }
            let rgb = rgb.map(gamma_u8_from_linear_f32);

            Color32::from_rgb(rgb[0], rgb[1], rgb[2])
        })
        .collect();

    ColorImage {
        size: [resolution, resolution],
        pixels,
    }
}

fn point_in_or_on_convex_polygon(points: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut i = 0;
    let mut j = points.len() - 1;
    let mut result = false;

    loop {
        if i >= points.len() {
            break;
        }

        let (x1, y1) = points[i].into();
        let (x2, y2) = points[j].into();

        if (x == x1 && y == y1) || (x == x2 && y == y2) {
            return true;
        }

        if (y1 > y) != (y2 > y) && (x < (x2 - x1) * (y - y1) / (y2 - y1) + x1) {
            result = !result;
        }

        j = i;
        i += 1;
    }

    result
}

const TARGET_BOX_LENGTH: f64 = 0.0075;
fn create_polygon_for_target_rgb(conv: ColorConversion, mut dst: [f32; 3]) -> ([f64; 2], Polygon) {
    conv.convert_float(&mut dst);

    let x = dst[0] as f64;
    let y = dst[1] as f64;

    let poly = Polygon::new(vec![
        [x + TARGET_BOX_LENGTH, y - TARGET_BOX_LENGTH],
        [x - TARGET_BOX_LENGTH, y - TARGET_BOX_LENGTH],
        [x - TARGET_BOX_LENGTH, y + TARGET_BOX_LENGTH],
        [x + TARGET_BOX_LENGTH, y + TARGET_BOX_LENGTH],
    ]);

    ([x, y], poly)
}
