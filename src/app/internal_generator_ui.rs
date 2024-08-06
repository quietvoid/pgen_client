use std::time::Duration;

use eframe::{
    egui::{self, Context, Layout, RichText, Sense, TextEdit, Ui},
    epaint::{vec2, Color32, Stroke, Vec2},
};
use egui_extras::{Column, TableBuilder};
use kolor_64::details::{color::WhitePoint, transform::XYZ_to_xyY};
use strum::IntoEnumIterator;

use crate::{
    calibration::{xyz_to_cct, TargetColorspace},
    external::ExternalJobCmd,
    generators::internal::PatchListPreset,
    utils::rgb_10b_to_8b,
};

use super::{
    status_color_active, utils::is_dragvalue_finished, CalibrationState, PGenApp, ReadFileType,
};

const PATCH_LIST_COLUMNS: &[&str] = &["#", "Patch", "Red", "Green", "Blue"];

pub fn add_internal_generator_ui(app: &mut PGenApp, ctx: &Context, ui: &mut Ui) {
    let cal_started = app.cal_state.internal_gen.started;

    ui.add_space(10.0);

    ui.heading("spotread CLI args");
    ui.add_enabled_ui(
        !app.cal_state.spotread_started && !cal_started && !app.processing,
        |ui| {
            add_spotread_cli_args_ui(app, ui);
        },
    );

    ui.add_space(5.0);
    add_spotread_status_ui(app, ctx, ui);

    ui.add_space(10.0);
    add_target_config_ui(app, ui);

    ui.heading("Patch list");
    ui.indent("patch_list_indent", |ui| {
        ui.horizontal(|ui| {
            let internal_gen = &mut app.cal_state.internal_gen;

            ui.label("Preset");
            ui.add_enabled_ui(!cal_started, |ui| {
                egui::ComboBox::from_id_source("patch_list_presets")
                    .selected_text(internal_gen.preset.as_ref())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for preset in PatchListPreset::iter() {
                            ui.selectable_value(&mut internal_gen.preset, preset, preset.as_ref());
                        }
                    });
                if ui.button("Load").clicked() {
                    internal_gen.load_preset(&app.state.pattern_config);
                }

                if ui.button("Load file").clicked() {
                    app.ctx
                        .external_tx
                        .try_send(ExternalJobCmd::ReadFile(ReadFileType::PatchList))
                        .ok();
                }
            });
        });

        ui.horizontal(|ui| {
            ui.add_enabled_ui(!cal_started, |ui| {
                ui.checkbox(
                    &mut app.cal_state.internal_gen.pattern_insertion_cfg.enabled,
                    "Full field pattern insertion",
                );

                let mut duration = app
                    .cal_state
                    .internal_gen
                    .pattern_insertion_cfg
                    .duration
                    .as_secs_f64();
                ui.label("Duration");
                ui.add(
                    egui::DragValue::new(&mut duration)
                        .update_while_editing(false)
                        .suffix(" s")
                        .max_decimals(2)
                        .speed(0.01)
                        .range(0.5..=10.0),
                );
                app.cal_state.internal_gen.pattern_insertion_cfg.duration =
                    Duration::from_secs_f64(duration);

                let mut level = app.cal_state.internal_gen.pattern_insertion_cfg.level * 100.0;
                ui.label("Level");
                ui.add(
                    egui::DragValue::new(&mut level)
                        .update_while_editing(false)
                        .suffix(" %")
                        .max_decimals(2)
                        .speed(0.1)
                        .range(0.0..=100.0),
                );
                app.cal_state.internal_gen.pattern_insertion_cfg.level = level / 100.0;
            });
        });

        ui.separator();

        let avail_height = ui.available_height();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_enabled_ui(!cal_started, |ui| {
                    add_patch_list_table(app, ui, avail_height);
                });
            });

            ui.vertical(|ui| {
                add_patches_info_right_side(app, ui);
            });
        });
    });
}

fn add_spotread_cli_args_ui(app: &mut PGenApp, ui: &mut Ui) {
    egui::Grid::new("spotread_cli_args_grid")
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            ui.strong("Key");
            ui.strong("Value");
            ui.label("");
            ui.end_row();

            for i in 0..=app.cal_state.spotread_cli_args.len() {
                let real_row = i < app.cal_state.spotread_cli_args.len();
                {
                    let args = if real_row {
                        &mut app.cal_state.spotread_cli_args[i]
                    } else {
                        &mut app.cal_state.spotread_tmp_args
                    };

                    ui.add_sized(Vec2::new(75.0, 20.0), TextEdit::singleline(&mut args.0));
                    let value_res = ui.add_sized(
                        Vec2::new(300.0, 20.0),
                        TextEdit::singleline(args.1.get_or_insert_with(Default::default)),
                    );

                    let is_enabled = {
                        let tmp_args = &app.cal_state.spotread_tmp_args;
                        real_row || (!tmp_args.0.is_empty() && tmp_args.1.is_some())
                    };
                    let add_value_changed = is_enabled && !real_row && value_res.lost_focus();

                    ui.add_enabled_ui(is_enabled, |ui| {
                        let btn_label = if real_row { "Remove" } else { "Add" };
                        if add_value_changed || ui.button(btn_label).clicked() {
                            if real_row {
                                app.cal_state.spotread_cli_args.remove(i);
                            } else {
                                let tmp_args = &mut app.cal_state.spotread_tmp_args;
                                app.cal_state.spotread_cli_args.push(tmp_args.clone());

                                tmp_args.0.clear();
                                tmp_args.1.take();
                            }
                        }
                    });

                    ui.end_row();
                }
            }
        });
}

fn add_spotread_status_ui(app: &mut PGenApp, ctx: &Context, ui: &mut Ui) {
    let spotread_started = app.cal_state.spotread_started;

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
                        .try_send(ExternalJobCmd::StartSpotreadProcess(
                            app.cal_state.spotread_cli_args.to_owned(),
                        ))
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
}

fn add_target_config_ui(app: &mut PGenApp, ui: &mut Ui) {
    let cal_started = app.cal_state.internal_gen.started;

    ui.horizontal(|ui| {
        ui.label("Target brightness");
        ui.add_enabled_ui(!cal_started, |ui| {
            ui.label("Min");
            let min_y_res = ui.add(
                egui::DragValue::new(&mut app.cal_state.min_y)
                    .update_while_editing(false)
                    .suffix(" nits")
                    .max_decimals(6)
                    .speed(0.0001)
                    .range(0.0..=5.0),
            );
            if is_dragvalue_finished(min_y_res) {
                app.cal_state.update_patterns_target();
            }

            ui.label("Max");
            let max_y_res = ui.add(
                egui::DragValue::new(&mut app.cal_state.max_y)
                    .update_while_editing(false)
                    .suffix(" nits")
                    .max_decimals(3)
                    .speed(0.1)
                    .range(25.0..=10_000.0),
            );
            if is_dragvalue_finished(max_y_res) {
                app.cal_state.update_patterns_target();
            }

            ui.label("Max HDR MDL");
            let max_mdl_res = ui.add(
                egui::DragValue::new(&mut app.cal_state.max_hdr_mdl)
                    .update_while_editing(false)
                    .suffix(" nits")
                    .max_decimals(3)
                    .speed(1.0)
                    .range(400.0..=10_000.0),
            );
            if is_dragvalue_finished(max_mdl_res) {
                app.cal_state.update_patterns_target();
            }
        });
    });

    ui.horizontal(|ui| {
        ui.label("Target primaries");

        let old_csp = app.cal_state.target_csp;
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
        if old_csp != app.cal_state.target_csp {
            app.cal_state.update_patterns_target();
        }
    });
}

fn add_patch_list_table(app: &mut PGenApp, ui: &mut Ui, avail_height: f32) {
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
        .min_scrolled_height(300.0_f32.max(avail_height - 25.0))
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

fn add_patches_info_right_side(app: &mut PGenApp, ui: &mut Ui) {
    let pgen_connected = app.state.connected_state.connected;
    let cal_started = app.cal_state.internal_gen.started;

    let can_read_patches = {
        let internal_gen = &app.cal_state.internal_gen;

        pgen_connected
            && app.cal_state.spotread_started
            && !app.processing
            && !cal_started
            && !internal_gen.list.is_empty()
    };
    let has_selected_patch = app.cal_state.internal_gen.selected_idx.is_some();

    ui.horizontal(|ui| {
        ui.add_enabled_ui(can_read_patches, |ui| {
            if ui.button("Measure patches").clicked() {
                {
                    let internal_gen = &mut app.cal_state.internal_gen;
                    internal_gen.started = true;
                    internal_gen.auto_advance = true;
                    internal_gen.selected_idx = Some(0);
                }

                app.calibration_send_measure_selected_patch();
            }
            if has_selected_patch {
                if ui.button("Measure selected patch").clicked() {
                    let internal_gen = &mut app.cal_state.internal_gen;
                    internal_gen.started = true;
                    internal_gen.auto_advance = false;

                    app.calibration_send_measure_selected_patch();
                }
                ui.checkbox(
                    &mut app.cal_state.internal_gen.read_selected_continuously,
                    "Continuous",
                );
            }
        });
    });

    let can_keep_reading = app.cal_state.internal_gen.auto_advance
        || app.cal_state.internal_gen.read_selected_continuously;
    let show_stop_btn = cal_started && can_keep_reading;
    if show_stop_btn && ui.button("Stop measuring").clicked() {
        let internal_gen = &mut app.cal_state.internal_gen;
        internal_gen.started = false;
        internal_gen.auto_advance = false;
    }

    let has_selected_patch_result = app
        .cal_state
        .internal_gen
        .selected_patch()
        .and_then(|e| e.result)
        .is_some();
    if has_selected_patch_result {
        ui.separator();
        add_selected_patch_results(ui, &mut app.cal_state);
    }
}

const XYY_RESULT_HEADERS: &[&str] = &["", "Target", "Actual", "Deviation"];
const XYY_RESULT_GRID: [(usize, &str); 3] = [(2, "Y"), (0, "x"), (1, "y")];
fn add_selected_patch_results(ui: &mut Ui, cal_state: &mut CalibrationState) {
    let res = cal_state
        .internal_gen
        .selected_patch()
        .and_then(|e| e.result.as_ref())
        .unwrap();

    let target_rgb_to_xyz = cal_state.target_rgb_to_xyz_conv();
    let target_xyz = res.ref_xyz_display_space(target_rgb_to_xyz, true);
    let target_xyy = XYZ_to_xyY(target_xyz, WhitePoint::D65);

    let actual_xyy = res.xyy;
    let xyy_dev = actual_xyy - target_xyy;

    let label_size = 20.0;
    let text_size = label_size - 2.0;
    let value_col = Column::auto().at_least(100.0);
    let cell_layout = Layout::default()
        .with_main_align(egui::Align::Max)
        .with_cross_align(egui::Align::Max);
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(cell_layout)
        .column(Column::auto().at_least(40.0))
        .column(value_col)
        .column(value_col)
        .column(value_col)
        .header(25.0, |mut header| {
            for label in XYY_RESULT_HEADERS.iter().copied() {
                header.col(|ui| {
                    ui.label(label);
                });
            }
        })
        .body(|mut body| {
            for (cmp, label) in XYY_RESULT_GRID {
                body.row(25.0, |mut row| {
                    let target_cmp = target_xyy[cmp];
                    let actual_cmp = actual_xyy[cmp];

                    let cmp_dev = xyy_dev[cmp];
                    let cmp_dev_str = if cal_state.show_deviation_percent {
                        let cmp_dev_pct = (cmp_dev / target_cmp.abs()) * 100.0;
                        format!("{cmp_dev_pct:.4} %")
                    } else {
                        format!("{cmp_dev:.4}")
                    };

                    row.col(|ui| {
                        ui.strong(RichText::new(label).size(label_size));
                    });
                    row.col(|ui| {
                        ui.strong(RichText::new(format!("{target_cmp:.4}")).size(text_size));
                    });
                    row.col(|ui| {
                        ui.strong(RichText::new(format!("{actual_cmp:.4}")).size(text_size));
                    });
                    row.col(|ui| {
                        ui.strong(RichText::new(cmp_dev_str).size(text_size));
                    });
                });
            }

            // CCT is only relevant for greyscale readings
            if res.is_white_stimulus_reading() {
                let target_cct = xyz_to_cct(target_xyz).unwrap_or_default();
                let actual_cct = res.cct;
                let cct_dev = actual_cct - target_cct;
                let cct_dev_str = if cal_state.show_deviation_percent {
                    let cct_dev_pct = (cct_dev / target_cct.abs()) * 100.0;
                    format!("{cct_dev_pct:.4} %")
                } else {
                    format!("{cct_dev:.4}")
                };

                body.row(25.0, |mut row| {
                    row.col(|ui| {
                        ui.strong(RichText::new("CCT").size(label_size));
                    });
                    row.col(|ui| {
                        ui.strong(RichText::new(format!("{target_cct:.4}")).size(text_size));
                    });
                    row.col(|ui| {
                        ui.strong(RichText::new(format!("{actual_cct:.4}")).size(text_size));
                    });
                    row.col(|ui| {
                        ui.strong(RichText::new(cct_dev_str).size(text_size));
                    });
                });
            }
        });

    ui.add_space(5.0);
    ui.checkbox(&mut cal_state.show_deviation_percent, "Deviation %");

    ui.separator();

    let actual_de2000 = res.delta_e2000(target_rgb_to_xyz);
    let actual_de2000_incl_lum = res.delta_e2000_incl_luminance(target_rgb_to_xyz);
    let actual_gamma_str = if let Some(actual_gamma) = res.gamma() {
        format!("{actual_gamma:.4}")
    } else {
        "N/A".to_string()
    };

    egui::Grid::new("selected_patch_delta_grid")
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            ui.strong(RichText::new("dE 2000").size(label_size));
            ui.strong(RichText::new("dE 2000 (w/ lum)").size(label_size));
            ui.strong(RichText::new("EOTF").size(label_size));
            ui.end_row();

            ui.strong(RichText::new(format!("{actual_de2000:.4}")).size(label_size));
            ui.strong(RichText::new(format!("{actual_de2000_incl_lum:.4}")).size(label_size));
            ui.strong(RichText::new(actual_gamma_str).size(label_size));
            ui.end_row();
        });
}
