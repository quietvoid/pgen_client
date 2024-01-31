use eframe::{
    egui::{self, Context, Layout, RichText, Sense, TextEdit, Ui},
    epaint::{vec2, Color32, Stroke, Vec2},
};
use egui_extras::{Column, TableBuilder};
use kolor_64::{
    details::{color::WhitePoint, transform::XYZ_to_xyY},
    ColorConversion,
};
use strum::IntoEnumIterator;

use crate::{
    calibration::{xyz_to_cct, TargetColorspace},
    external::ExternalJobCmd,
    generators::internal::PatchListPreset,
    utils::rgb_10b_to_8b,
};

use super::{status_color_active, CalibrationState, PGenApp};

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
                    .width(200.0)
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

        let avail_height = ui.available_height();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_enabled_ui(!cal_started, |ui| {
                    add_patch_list_table(app, ui, avail_height);
                });
            });

            add_patches_info_right_side(app, ui);
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

            let actual_len = app.cal_state.spotread_cli_args.len();
            for i in 0..=actual_len {
                let real_row = i < actual_len;
                {
                    let args = if real_row {
                        &mut app.cal_state.spotread_cli_args[i]
                    } else {
                        &mut app.cal_state.spotread_tmp_args
                    };

                    ui.add_sized(Vec2::new(75.0, 20.0), TextEdit::singleline(&mut args.0));
                    let value_res =
                        ui.add_sized(Vec2::new(300.0, 20.0), TextEdit::singleline(&mut args.1));

                    let is_enabled = {
                        let tmp_args = &app.cal_state.spotread_tmp_args;
                        real_row || (!tmp_args.0.is_empty() && !tmp_args.1.is_empty())
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
                                tmp_args.1.clear();
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
        .min_scrolled_height(avail_height - 25.0)
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

    ui.vertical(|ui| {
        let can_read_patches = {
            let internal_gen = &app.cal_state.internal_gen;

            pgen_connected
                && app.cal_state.spotread_started
                && !app.processing
                && !cal_started
                && !internal_gen.list.is_empty()
        };

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

                let has_selected_patch = app.cal_state.internal_gen.selected_idx.is_some();
                if has_selected_patch {
                    ui.add_space(5.0);
                    if ui.button("Measure selected patch").clicked() {
                        let internal_gen = &mut app.cal_state.internal_gen;
                        internal_gen.started = true;
                        internal_gen.auto_advance = false;

                        app.calibration_send_measure_selected_patch();
                    }
                }
            });
        });

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
    });
}

const XYY_RESULT_HEADERS: &[&str] = &["", "Target", "Actual", "Deviation"];
const XYY_RESULT_GRID: [(usize, &str); 3] = [(2, "Y"), (0, "x"), (1, "y")];
fn add_selected_patch_results(ui: &mut Ui, cal_state: &mut CalibrationState) {
    let res = cal_state
        .internal_gen
        .selected_patch()
        .and_then(|e| e.result.as_ref())
        .unwrap();

    // All RGB values are the same, needs to calculate with BPC
    let grayscale = {
        let first = res.target.ref_rgb.x;
        res.target.ref_rgb.to_array().iter().all(|e| *e == first)
    };

    let minmax_y = grayscale
        .then(|| cal_state.internal_gen.minmax_y())
        .flatten();

    let target_rgb_to_xyz =
        ColorConversion::new(cal_state.target_csp.to_kolor(), kolor_64::spaces::CIE_XYZ);
    let target_xyz = res.ref_xyz(minmax_y, target_rgb_to_xyz, cal_state.eotf);

    let target_xyy = XYZ_to_xyY(target_xyz, WhitePoint::D65);
    let target_cct = xyz_to_cct(target_xyz).unwrap_or_default();

    let actual_xyy = res.xyy;
    let xyy_dev = actual_xyy - target_xyy;

    let label_size = 20.0;
    let text_size = label_size - 2.0;
    let value_col = Column::auto().at_least(80.0);
    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto().at_least(20.0))
        .column(value_col)
        .column(value_col)
        .column(value_col)
        .cell_layout(
            Layout::centered_and_justified(egui::Direction::LeftToRight)
                .with_cross_align(egui::Align::Max),
        )
        .header(20.0, |mut header| {
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
        });

    ui.add_space(15.0);
    ui.checkbox(&mut cal_state.show_deviation_percent, "Deviation %");
}
