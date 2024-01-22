use eframe::{
    egui::{self, Context, Sense, Ui},
    epaint::{vec2, Color32, Stroke, Vec2},
};
use egui_extras::{Column, TableBuilder};
use strum::IntoEnumIterator;

use crate::{
    calibration::TargetColorspace, external::ExternalJobCmd, generators::internal::PatchListPreset,
    utils::rgb_10b_to_8b,
};

use super::{status_color_active, PGenApp};

const PATCH_LIST_COLUMNS: &[&str] = &["#", "Patch", "Red", "Green", "Blue"];

pub fn add_internal_generator_ui(app: &mut PGenApp, ctx: &Context, ui: &mut Ui) {
    let pgen_connected = app.state.connected_state.connected;
    let spotread_started = app.cal_state.spotread_started;
    let cal_started = app.cal_state.internal_gen.started;

    ui.horizontal(|ui| {
        let btn_label = if spotread_started {
            "Stop spotread"
        } else {
            "Start spotread"
        };
        ui.add_enabled_ui(!app.processing, |ui| {
            if ui.button(btn_label).clicked() {
                app.cal_state.internal_gen.started = false;

                if spotread_started {
                    app.ctx
                        .external_tx
                        .try_send(ExternalJobCmd::StopSpotreadProcess)
                        .ok();
                } else {
                    app.ctx
                        .external_tx
                        .try_send(ExternalJobCmd::StartSpotreadProcess)
                        .ok();
                }
            }
        });
        let status_color = status_color_active(ctx, spotread_started);
        let (res, painter) = ui.allocate_painter(Vec2::new(16.0, 16.0), Sense::hover());
        painter.circle(res.rect.center(), 8.0, status_color, Stroke::NONE);

        // Show patch/list progress
        let current_idx = app
            .cal_state
            .internal_gen
            .started
            .then_some(app.cal_state.internal_gen.selected_idx)
            .flatten();
        if let Some(idx) = current_idx {
            if app.cal_state.internal_gen.auto_advance {
                let num = (idx + 1) as f32;
                let count = app.cal_state.internal_gen.list.len() as f32;

                let progress = num / count;
                let pb = egui::ProgressBar::new(progress)
                    .animate(true)
                    .text(format!("Measuring: Patch {num} / {count}"));
                ui.add(pb);
            } else {
                ui.strong(format!("Measuring patch {idx}"));
                ui.add(egui::Spinner::new());
            }
        }
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label("Target");
        ui.add_enabled_ui(!cal_started, |ui| {
            egui::ComboBox::from_id_source("target_colorspaces")
                .selected_text(app.cal_state.target_csp.as_ref())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for csp in TargetColorspace::iter() {
                        ui.selectable_value(&mut app.cal_state.target_csp, csp, csp.as_ref());
                    }
                });
        });
    });

    ui.heading("Patch list");
    ui.indent("patch_list_indent", |ui| {
        ui.horizontal(|ui| {
            let internal_gen = &mut app.cal_state.internal_gen;

            ui.label("Preset");
            ui.add_enabled_ui(!cal_started, |ui| {
                egui::ComboBox::from_id_source("patch_list_presets")
                    .selected_text(internal_gen.preset.as_ref())
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for preset in PatchListPreset::iter() {
                            ui.selectable_value(&mut internal_gen.preset, preset, preset.as_ref());
                        }
                    });
                if ui.button("Load").clicked() {
                    internal_gen.load_preset(&app.state.pattern_config);
                }
            });

            /* TODO
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Load file").clicked() {
                }
            })
            */
        });

        ui.separator();
        ui.add_enabled_ui(!cal_started, |ui| {
            add_patch_list_table(app, ui);
        });
    });
    ui.add_space(5.0);

    let can_read_patches = {
        let internal_gen = &app.cal_state.internal_gen;

        pgen_connected
            && spotread_started
            && !app.processing
            && !cal_started
            && !internal_gen.list.is_empty()
    };
    ui.add_enabled_ui(can_read_patches, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Measure patches").clicked() {
                {
                    let internal_gen = &mut app.cal_state.internal_gen;
                    internal_gen.started = true;
                    internal_gen.auto_advance = true;
                    internal_gen.selected_idx = Some(0);
                }

                app.calibration_send_measure_selected_patch();
            }

            let has_selected_patch = app.cal_state.internal_gen.selected_idx.is_some();
            if has_selected_patch && ui.button("Measure selected patch").clicked() {
                {
                    let internal_gen = &mut app.cal_state.internal_gen;
                    internal_gen.started = true;
                    internal_gen.auto_advance = false;
                }

                app.calibration_send_measure_selected_patch();
            }
        });
    });
}

fn add_patch_list_table(app: &mut PGenApp, ui: &mut Ui) {
    let bit_depth = app.state.pattern_config.bit_depth as u8;

    let internal_gen = &mut app.cal_state.internal_gen;
    let rows = &internal_gen.list;

    let patch_col = Column::auto().at_least(50.0);
    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto().at_least(25.0))
        .column(patch_col)
        .column(patch_col)
        .column(patch_col)
        .column(patch_col)
        .resizable(true)
        .min_scrolled_height(0.0)
        .sense(Sense::click())
        .header(20.0, |mut header| {
            for label in PATCH_LIST_COLUMNS.iter().copied() {
                header.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, rows.len(), |mut row| {
                let i = row.index();
                row.set_selected(internal_gen.selected_idx.is_some_and(|si| i == si));

                let patch = &rows[i];

                let rgb_orig = patch.rgb;
                let rgb_8b = rgb_10b_to_8b(bit_depth, rgb_orig);
                let patch_colour = Color32::from_rgb(rgb_8b[0], rgb_8b[1], rgb_8b[2]);

                row.col(|ui| {
                    ui.label(i.to_string());
                });
                row.col(|ui| {
                    ui.add_space(2.0);
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), ui.available_height() - 2.0),
                        Sense::hover(),
                    );
                    ui.painter()
                        .rect(rect, 0.0, patch_colour, Stroke::new(1.0, Color32::BLACK));
                });
                for c in rgb_orig {
                    row.col(|ui| {
                        ui.label(format!("{c}"));
                    });
                }

                if row.response().clicked() {
                    if internal_gen.selected_idx.is_some_and(|si| i == si) {
                        internal_gen.selected_idx.take();
                    } else {
                        internal_gen.selected_idx.replace(i);
                    }
                }
            })
        });
}
