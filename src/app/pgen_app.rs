use std::net::{IpAddr, SocketAddr};

use eframe::egui::{self, Layout, Sense};
use eframe::epaint::{Color32, Stroke, Vec2};
use strum::IntoEnumIterator;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::generators::{GeneratorClient, GeneratorCmd, GeneratorState};
use crate::pgen::commands::{PGenCommand, PGenSetConfCommand};
use crate::pgen::controller::{PGenControllerCmd, PGenControllerState, PGenInfo, PGenOutputConfig};
use crate::pgen::pattern_config::{TestPatternPosition, TestPatternSize};
use crate::pgen::utils::{
    compute_rgb_range, rgb_10b_to_8b, scale_8b_rgb_to_10b, scale_pattern_config_rgb_values,
};
use crate::pgen::{
    BitDepth, ColorFormat, Colorimetry, DoviMapMode, DynamicRange, HdrEotf, Primaries, QuantRange,
};

pub use super::{PGenAppContext, PGenAppSavedState, PGenAppUpdate};

pub struct PGenApp {
    pub ctx: PGenAppContext,
    pub state: PGenControllerState,
    pub editing_socket: (String, String),
    pub generator_state: GeneratorState,

    pub processing: bool,
    pub requested_close: bool,
    pub allowed_to_close: bool,
}

impl PGenApp {
    pub fn new(
        rx: Receiver<PGenAppUpdate>,
        controller_tx: Sender<PGenControllerCmd>,
        generator_tx: Sender<GeneratorCmd>,
    ) -> Self {
        let ctx = PGenAppContext {
            rx,
            controller_tx,
            generator_tx,
        };

        let state: PGenControllerState = Default::default();
        let socket_addr = state.socket_addr;

        Self {
            ctx,
            state,
            editing_socket: (socket_addr.ip().to_string(), socket_addr.port().to_string()),
            generator_state: GeneratorState {
                client: GeneratorClient::Resolve,
                listening: false,
            },
            processing: false,
            requested_close: false,
            allowed_to_close: false,
        }
    }

    pub fn close(&mut self) {
        log::info!("Requested close, disconnecting");
        self.requested_close = true;

        // Force message to be sent
        let controller_tx = self.ctx.controller_tx.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                controller_tx.send(PGenControllerCmd::Disconnect).await.ok();
            });
        });
    }

    pub(crate) fn check_responses(&mut self) {
        while let Ok(msg) = self.ctx.rx.try_recv() {
            match msg {
                PGenAppUpdate::GeneratorListening(v) => {
                    log::debug!("Generator listening: {v}");
                    self.generator_state.listening = v
                }
                PGenAppUpdate::InitialSetup {
                    egui_ctx,
                    saved_state,
                } => {
                    self.ctx
                        .controller_tx
                        .try_send(PGenControllerCmd::SetGuiCallback(egui_ctx))
                        .ok();

                    if let Some(saved_state) = saved_state {
                        self.update_from_new_state(saved_state.state);
                        self.editing_socket = saved_state.editing_socket;
                        self.generator_state = saved_state.generator_state;

                        self.ctx
                            .controller_tx
                            .try_send(PGenControllerCmd::SetInitialState(self.state.clone()))
                            .ok();
                    }
                }
                PGenAppUpdate::NewState(state) => self.update_from_new_state(state),
                PGenAppUpdate::Processing => self.processing = true,
                PGenAppUpdate::DoneProcessing => self.processing = false,
            }
        }

        if self.requested_close && !self.processing {
            self.allowed_to_close = true;
            self.ctx.rx.close();
        }
    }

    pub(crate) fn set_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                egui::widgets::global_dark_light_mode_switch(ui);
                if self.processing {
                    ui.add(egui::Spinner::new().size(26.0));
                }
                ui.separator();

                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Exit").clicked() {
                            self.close();
                        }
                    });
                })
            });
        });
    }

    pub(crate) fn set_central_panel(&mut self, ctx: &egui::Context) {
        let can_edit_configs = !self.processing && !self.generator_state.listening;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.add_default_config(ctx, ui);
            ui.separator();

            ui.add_enabled_ui(can_edit_configs, |ui| {
                self.add_output_info(ui);
                self.add_pattern_config_grid(ui);
            });
            ui.separator();

            ui.add_enabled_ui(!self.processing, |ui| {
                self.add_generator_config(ctx, ui);
            });
        });
    }

    fn add_default_config(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let connected = self.state.connected_state.connected;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.label("IP Address");
            let ip_res =
                ui.add(egui::TextEdit::singleline(&mut self.editing_socket.0).desired_width(255.0));

            ui.label("Port");
            let port_res =
                ui.add(egui::TextEdit::singleline(&mut self.editing_socket.1).desired_width(50.0));

            if ip_res.lost_focus() || port_res.lost_focus() {
                let parsed_ip = self.editing_socket.0.parse::<IpAddr>();
                let parsed_port = self.editing_socket.1.parse::<u16>();

                if let (Ok(new_ip), Ok(new_port)) = (&parsed_ip, &parsed_port) {
                    let new_socket: SocketAddr = SocketAddr::new(*new_ip, *new_port);
                    if self.state.socket_addr != new_socket {
                        self.state.socket_addr = new_socket;

                        self.ctx
                            .controller_tx
                            .try_send(PGenControllerCmd::UpdateSocket(self.state.socket_addr))
                            .ok();
                    }
                } else {
                    // Clear invalid back to current socket
                    if parsed_ip.is_err() {
                        self.editing_socket.0 = self.state.socket_addr.ip().to_string();
                    }
                    if parsed_port.is_err() {
                        self.editing_socket.1 = self.state.socket_addr.port().to_string();
                    }
                }
            }
        });

        ui.add_enabled_ui(!self.processing, |ui| {
            egui::Grid::new("prefs_grid")
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Status");

                    let status_str = if connected {
                        "Connected"
                    } else if let Some(err) = &self.state.connected_state.error {
                        err.as_str()
                    } else {
                        "Not connected"
                    };

                    ui.label(status_str);

                    let status_color = if connected {
                        if ctx.style().visuals.dark_mode {
                            Color32::DARK_GREEN
                        } else {
                            Color32::LIGHT_GREEN
                        }
                    } else if ctx.style().visuals.dark_mode {
                        Color32::DARK_RED
                    } else {
                        Color32::LIGHT_RED
                    };
                    let (res, painter) = ui.allocate_painter(Vec2::new(16.0, 16.0), Sense::hover());
                    painter.circle(res.rect.center(), 8.0, status_color, Stroke::NONE);

                    if ui.button("Connect").clicked() {
                        self.ctx
                            .controller_tx
                            .try_send(PGenControllerCmd::InitialConnect)
                            .ok();
                    }

                    if connected {
                        if ui.button("Disconnect").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::Disconnect)
                                .ok();
                        }

                        if ui.button("Shutdown device").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::PGen(PGenCommand::Shutdown))
                                .ok();
                        }

                        if ui.button("Reboot device").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::PGen(PGenCommand::Reboot))
                                .ok();
                        }
                    }
                });

            if let Some(info) = connected.then_some(self.state.pgen_info.as_ref()).flatten() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Version: {}, {}", info.version, info.pid));

                    if ui.button("Restart PGenerator software").clicked() {
                        restart_pgenerator_sw(&self.ctx);
                    }
                });
            }
        });
    }

    fn add_output_info(&mut self, ui: &mut egui::Ui) {
        let output_config = self
            .state
            .connected_state
            .connected
            .then_some(self.state.pgen_info.as_mut())
            .flatten();

        if let Some(pgen_info) = output_config {
            ui.horizontal(|ui| {
                Self::add_base_output_config(&self.ctx, pgen_info, ui);

                let output_cfg = &mut pgen_info.output_config;
                ui.add_enabled_ui(output_cfg.dynamic_range != DynamicRange::Dovi, |ui| {
                    Self::add_hdr_output_config(&self.ctx, output_cfg, ui);
                });
            });

            ui.separator();
        }
    }

    fn add_base_output_config(ctx: &PGenAppContext, pgen_info: &mut PGenInfo, ui: &mut egui::Ui) {
        let output_cfg = &mut pgen_info.output_config;
        let is_dovi = output_cfg.dynamic_range == DynamicRange::Dovi;

        ui.vertical(|ui| {
            ui.heading("Output config / AVI infoframe");
            ui.indent("pgen_output_config", |ui| {
                egui::Grid::new("output_config_grid")
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        let old_display_mode = pgen_info.current_display_mode;
                        ui.label("Display mode");
                        ui.add_enabled_ui(!is_dovi, |ui| {
                            egui::ComboBox::from_id_source(egui::Id::new("out_display_mode"))
                                .width(200.0)
                                .selected_text(pgen_info.current_display_mode.to_string())
                                .show_ui(ui, |ui| {
                                    for mode in pgen_info.display_modes.iter().copied() {
                                        ui.selectable_value(
                                            &mut pgen_info.current_display_mode,
                                            mode,
                                            mode.to_string(),
                                        );
                                    }
                                });
                        });
                        ui.end_row();
                        if pgen_info.current_display_mode != old_display_mode {
                            log::debug!("Change mode to {}", pgen_info.current_display_mode);
                            ctx.controller_tx
                                .try_send(PGenControllerCmd::ChangeDisplayMode(
                                    pgen_info.current_display_mode,
                                ))
                                .ok();
                        }

                        let old_format = output_cfg.format;
                        ui.label("Color format");
                        ui.add_enabled_ui(!is_dovi, |ui| {
                            egui::ComboBox::from_id_source(egui::Id::new("out_color_format"))
                                .width(125.0)
                                .selected_text(output_cfg.format.as_ref())
                                .show_ui(ui, |ui| {
                                    for format in ColorFormat::iter() {
                                        ui.selectable_value(
                                            &mut output_cfg.format,
                                            format,
                                            format.as_ref(),
                                        );
                                    }
                                });
                        });
                        ui.end_row();
                        if output_cfg.format != old_format {
                            update_output_color_format(ctx, output_cfg.format);
                        }

                        let old_quant_range = output_cfg.quant_range;
                        ui.label("Quant range");
                        ui.add_enabled_ui(
                            output_cfg.format == ColorFormat::Rgb && !is_dovi,
                            |ui| {
                                egui::ComboBox::from_id_source(egui::Id::new("out_quant_range"))
                                    .width(125.0)
                                    .selected_text(output_cfg.quant_range.as_ref())
                                    .show_ui(ui, |ui| {
                                        for quant_range in QuantRange::iter() {
                                            ui.selectable_value(
                                                &mut output_cfg.quant_range,
                                                quant_range,
                                                quant_range.as_ref(),
                                            );
                                        }
                                    });
                            },
                        );
                        ui.end_row();
                        if output_cfg.quant_range != old_quant_range {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetQuantRange(output_cfg.quant_range),
                            );
                        }

                        let old_bit_depth = output_cfg.bit_depth;
                        ui.label("Bit depth");
                        ui.add_enabled_ui(!is_dovi, |ui| {
                            egui::ComboBox::from_id_source(egui::Id::new("out_max_bpc"))
                                .width(125.0)
                                .selected_text(output_cfg.bit_depth.as_ref())
                                .show_ui(ui, |ui| {
                                    for depth in BitDepth::iter() {
                                        ui.selectable_value(
                                            &mut output_cfg.bit_depth,
                                            depth,
                                            depth.as_ref(),
                                        );
                                    }
                                });
                        });
                        ui.end_row();
                        if output_cfg.bit_depth != old_bit_depth {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetBitDepth(output_cfg.bit_depth),
                            );
                        }

                        let old_colorimetry = output_cfg.colorimetry;
                        ui.label("Colorimetry");
                        ui.add_enabled_ui(!is_dovi, |ui| {
                            egui::ComboBox::from_id_source(egui::Id::new("out_colorimetry"))
                                .width(125.0)
                                .selected_text(output_cfg.colorimetry.as_ref())
                                .show_ui(ui, |ui| {
                                    for colorimetry in Colorimetry::iter() {
                                        ui.selectable_value(
                                            &mut output_cfg.colorimetry,
                                            colorimetry,
                                            colorimetry.as_ref(),
                                        );
                                    }
                                });
                        });
                        ui.end_row();
                        if output_cfg.colorimetry != old_colorimetry {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetColorimetry(output_cfg.colorimetry),
                            );
                        }

                        let old_dynamic_range = output_cfg.dynamic_range;
                        ui.label("Dynamic range");
                        egui::ComboBox::from_id_source(egui::Id::new("out_dynamic_range"))
                            .width(125.0)
                            .selected_text(output_cfg.dynamic_range.as_ref())
                            .show_ui(ui, |ui| {
                                for dynamic_range in DynamicRange::iter() {
                                    ui.selectable_value(
                                        &mut output_cfg.dynamic_range,
                                        dynamic_range,
                                        dynamic_range.as_ref(),
                                    );
                                }
                            });
                        ui.end_row();
                        if output_cfg.dynamic_range != old_dynamic_range {
                            ctx.controller_tx
                                .try_send(PGenControllerCmd::UpdateDynamicRange(
                                    output_cfg.dynamic_range,
                                ))
                                .ok();
                        }

                        if is_dovi {
                            let old_dovi_map_mode = output_cfg.dovi_map_mode;
                            ui.label("DoVi mode");
                            egui::ComboBox::from_id_source(egui::Id::new("out_dovi_map_mode"))
                                .width(125.0)
                                .selected_text(output_cfg.dovi_map_mode.as_ref())
                                .show_ui(ui, |ui| {
                                    for dovi_map_mode in DoviMapMode::iter() {
                                        ui.selectable_value(
                                            &mut output_cfg.dovi_map_mode,
                                            dovi_map_mode,
                                            dovi_map_mode.as_ref(),
                                        );
                                    }
                                });
                            ui.end_row();
                            if output_cfg.dovi_map_mode != old_dovi_map_mode {
                                send_pgen_set_conf_command(
                                    ctx,
                                    PGenSetConfCommand::SetDoviMapMode(output_cfg.dovi_map_mode),
                                );
                            }
                        }

                        ui.vertical(|ui| {
                            ui.add_space(20.0);
                        });
                        ui.end_row();

                        ui.label("");
                        ui.vertical_centered_justified(|ui| {
                            let set_btn = egui::Button::new("Set AVI infoframe")
                                .min_size((150.0, 20.0).into());
                            if ui.add(set_btn).clicked() {
                                restart_pgenerator_sw(ctx);
                            }
                        });
                        ui.end_row();
                    });
            });
        });
    }

    fn add_hdr_output_config(
        ctx: &PGenAppContext,
        output_cfg: &mut PGenOutputConfig,
        ui: &mut egui::Ui,
    ) {
        ui.vertical(|ui| {
            let hdr = &mut output_cfg.hdr_meta;
            ui.heading("HDR metadata / DRM infoframe");
            ui.indent("hdr_metadata_config", |ui| {
                egui::Grid::new("output_config_grid")
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        let old_eotf = hdr.eotf;
                        ui.label("EOTF");
                        egui::ComboBox::from_id_source(egui::Id::new("hdr_eotf"))
                            .width(200.0)
                            .selected_text(hdr.eotf.as_ref())
                            .show_ui(ui, |ui| {
                                for eotf in HdrEotf::iter() {
                                    ui.selectable_value(&mut hdr.eotf, eotf, eotf.as_ref());
                                }
                            });
                        ui.end_row();
                        if hdr.eotf != old_eotf {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetHdrEotf(hdr.eotf),
                            );
                        }

                        let old_primaries = hdr.primaries;
                        ui.label("Primaries");
                        egui::ComboBox::from_id_source(egui::Id::new("hdr_primaries"))
                            .width(200.0)
                            .selected_text(hdr.primaries.as_ref())
                            .show_ui(ui, |ui| {
                                for primaries in Primaries::iter() {
                                    ui.selectable_value(
                                        &mut hdr.primaries,
                                        primaries,
                                        primaries.as_ref(),
                                    );
                                }
                            });
                        ui.end_row();
                        if hdr.primaries != old_primaries {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetHdrPrimaries(hdr.primaries),
                            );
                        }

                        ui.label("Max MDL");
                        let max_mdl_res = ui.add(
                            egui::DragValue::new(&mut hdr.max_mdl)
                                .update_while_editing(false)
                                .suffix(" nits")
                                .max_decimals(0)
                                .clamp_range(0..=10_000),
                        );
                        ui.end_row();
                        if is_dragvalue_finished(max_mdl_res) {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetHdrMaxMdl(hdr.max_mdl),
                            );
                        }

                        let mut min_mdl = hdr.min_mdl as f64 / 10_000.0;
                        ui.label("Min MDL");
                        let min_mdl_res = ui.add(
                            egui::DragValue::new(&mut min_mdl)
                                .update_while_editing(false)
                                .suffix(" nits")
                                .max_decimals(5)
                                .speed(0.0001)
                                .clamp_range(0.0001..=0.0050),
                        );
                        hdr.min_mdl = (min_mdl * 10_000.0).round() as u16;
                        ui.end_row();
                        if is_dragvalue_finished(min_mdl_res) {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetHdrMinMdl(hdr.min_mdl),
                            );
                        }

                        ui.label("MaxCLL");
                        let maxcll_res = ui.add(
                            egui::DragValue::new(&mut hdr.maxcll)
                                .update_while_editing(false)
                                .suffix(" nits")
                                .max_decimals(0)
                                .clamp_range(0..=10_000),
                        );
                        ui.end_row();
                        if is_dragvalue_finished(maxcll_res) {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetHdrMaxCLL(hdr.maxcll),
                            );
                        }

                        ui.label("MaxFALL");
                        let maxfall_res = ui.add(
                            egui::DragValue::new(&mut hdr.maxfall)
                                .update_while_editing(false)
                                .suffix(" nits")
                                .max_decimals(0)
                                .clamp_range(0..=10_000),
                        );
                        ui.end_row();
                        if is_dragvalue_finished(maxfall_res) {
                            send_pgen_set_conf_command(
                                ctx,
                                PGenSetConfCommand::SetHdrMaxFALL(hdr.maxfall),
                            );
                        }

                        ui.vertical(|ui| {
                            ui.add_space(20.0);
                        });
                        ui.end_row();

                        ui.label("");
                        ui.vertical_centered_justified(|ui| {
                            if ui.button("Set DRM infoframe").clicked() {
                                restart_pgenerator_sw(ctx);
                            }
                        });
                        ui.end_row();
                    });
            });
        });
    }

    fn add_pattern_config_grid(&mut self, ui: &mut egui::Ui) {
        let connected = self.state.connected_state.connected;
        let old_limited_range = self.state.pattern_config.limited_range;
        let old_depth = self.state.pattern_config.bit_depth as u8;
        let old_preset_size = self.state.pattern_config.preset_size;
        let old_preset_position = self.state.pattern_config.preset_position;
        let rgb_range = compute_rgb_range(old_limited_range, old_depth);

        // Color picker values in 8 bit sRGB
        let old_rgb = rgb_10b_to_8b(old_depth, self.state.pattern_config.patch_colour);
        let mut rgb = old_rgb;

        let old_bg_rgb = rgb_10b_to_8b(old_depth, self.state.pattern_config.background_colour);
        let mut bg_rgb = old_bg_rgb;

        // Drag value raw values
        let old_rgb_dragv = self.state.pattern_config.patch_colour;
        let mut rgb_dragv = old_rgb_dragv;

        let old_bg_rgb_dragv = self.state.pattern_config.background_colour;
        let mut bg_rgb_dragv = old_bg_rgb_dragv;

        let mut state_updated = false;

        egui::Grid::new("pattern_conf_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                ui.label("Limited range");
                ui.add(egui::Checkbox::without_text(
                    &mut self.state.pattern_config.limited_range,
                ));
                ui.end_row();

                ui.label("Patch precision");
                egui::ComboBox::from_id_source(egui::Id::new("patch_depth_select"))
                    .width(75.0)
                    .selected_text(self.state.pattern_config.bit_depth.as_ref())
                    .show_ui(ui, |ui| {
                        for depth in BitDepth::iter() {
                            ui.selectable_value(
                                &mut self.state.pattern_config.bit_depth,
                                depth,
                                depth.as_ref(),
                            );
                        }
                    });

                ui.end_row();

                let pattern_size_info = connected
                    .then_some(self.compute_max_pattern_size_and_position())
                    .flatten();

                ui.label("Patch size");
                egui::ComboBox::from_id_source(egui::Id::new("preset_size_select"))
                    .selected_text(self.state.pattern_config.preset_size.as_ref())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);

                        for preset_size in TestPatternSize::iter() {
                            ui.selectable_value(
                                &mut self.state.pattern_config.preset_size,
                                preset_size,
                                preset_size.as_ref(),
                            );
                        }
                    });

                if let Some(size_info) = pattern_size_info {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.patch_size.0)
                                .update_while_editing(false)
                                .clamp_range(0..=size_info.0),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.patch_size.1)
                                .update_while_editing(false)
                                .clamp_range(0..=size_info.1),
                        );
                    });
                }
                ui.end_row();

                ui.label("Position");
                egui::ComboBox::from_id_source(egui::Id::new("preset_position_select"))
                    .selected_text(self.state.pattern_config.preset_position.as_ref())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);
                        for preset_pos in TestPatternPosition::iter() {
                            ui.selectable_value(
                                &mut self.state.pattern_config.preset_position,
                                preset_pos,
                                preset_pos.as_ref(),
                            );
                        }
                    });

                if let Some(size_info) = pattern_size_info {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.position.0)
                                .update_while_editing(false)
                                .clamp_range(0..=size_info.2),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.position.1)
                                .update_while_editing(false)
                                .clamp_range(0..=size_info.3),
                        );
                    });
                }
                ui.end_row();

                ui.label("Patch colour");
                ui.centered_and_justified(|ui| {
                    ui.color_edit_button_srgb(&mut rgb);
                });
                ui.horizontal(|ui| {
                    rgb_dragv.iter_mut().for_each(|c| {
                        ui.add(
                            egui::DragValue::new(c)
                                .update_while_editing(false)
                                .clamp_range(rgb_range.clone()),
                        );
                    });
                });
                ui.end_row();

                ui.label("Background colour");
                ui.centered_and_justified(|ui| {
                    ui.color_edit_button_srgb(&mut bg_rgb);
                });
                ui.horizontal(|ui| {
                    bg_rgb_dragv.iter_mut().for_each(|c| {
                        ui.add(
                            egui::DragValue::new(c)
                                .update_while_editing(false)
                                .clamp_range(rgb_range.clone()),
                        );
                    });
                });
                ui.end_row();

                ui.add_enabled_ui(connected, |ui| {
                    if ui.button("Send pattern").clicked() {
                        self.ctx
                            .controller_tx
                            .try_send(PGenControllerCmd::SendCurrentPattern)
                            .ok();
                    }

                    if ui.button("Blank pattern").clicked() {
                        self.ctx
                            .controller_tx
                            .try_send(PGenControllerCmd::SetBlank)
                            .ok();
                    }
                });
                ui.end_row();
            });

        let new_depth = self.state.pattern_config.bit_depth as u8;
        let new_limited_range = self.state.pattern_config.limited_range;
        if old_depth != new_depth || old_limited_range != new_limited_range {
            scale_pattern_config_rgb_values(
                &mut self.state.pattern_config,
                new_depth,
                old_depth,
                new_limited_range,
                old_limited_range,
            );
            state_updated |= true;
        }
        if old_preset_size != self.state.pattern_config.preset_size
            || old_preset_position != self.state.pattern_config.preset_position
        {
            self.state.set_pattern_size_and_pos_from_resolution();
            state_updated |= true;
        }
        if old_rgb != rgb {
            self.set_config_colour_from_srgb(self.state.pattern_config.bit_depth as u8, rgb, false);
            state_updated |= true;
        }
        if old_bg_rgb != bg_rgb {
            self.set_config_colour_from_srgb(
                self.state.pattern_config.bit_depth as u8,
                bg_rgb,
                true,
            );
            state_updated |= true;
        }
        if old_rgb_dragv != rgb_dragv {
            self.state.pattern_config.patch_colour = rgb_dragv;
            state_updated |= true;
        }
        if old_bg_rgb_dragv != bg_rgb_dragv {
            self.state.pattern_config.background_colour = bg_rgb_dragv;
            state_updated |= true;
        }

        if state_updated {
            self.update_controller_state();
        }
    }

    fn add_generator_config(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.with_layout(Layout::left_to_right(egui::Align::Min), |ui| {
            ui.label("Generator client");
            ui.add_enabled_ui(!self.generator_state.listening, |ui| {
                egui::ComboBox::from_id_source(egui::Id::new("generator_client"))
                    .selected_text(self.generator_state.client.as_ref())
                    .show_ui(ui, |ui| {
                        for client in GeneratorClient::iter() {
                            ui.selectable_value(
                                &mut self.generator_state.client,
                                client,
                                client.as_ref(),
                            );
                        }
                    });
            });

            let generator_label = if self.generator_state.listening {
                "Stop generator client"
            } else {
                "Start generator client"
            };
            let status_color = if self.generator_state.listening {
                if ctx.style().visuals.dark_mode {
                    Color32::DARK_GREEN
                } else {
                    Color32::LIGHT_GREEN
                }
            } else if ctx.style().visuals.dark_mode {
                Color32::DARK_RED
            } else {
                Color32::LIGHT_RED
            };
            ui.add_enabled_ui(self.state.connected_state.connected, |ui| {
                if ui.button(generator_label).clicked() {
                    let cmd = if self.generator_state.listening {
                        GeneratorCmd::StopClient(self.generator_state.client)
                    } else {
                        GeneratorCmd::StartClient(self.generator_state.client)
                    };

                    self.ctx.generator_tx.try_send(cmd).ok();
                }

                let (res, painter) = ui.allocate_painter(Vec2::new(16.0, 16.0), Sense::hover());
                painter.circle(res.rect.center(), 8.0, status_color, Stroke::NONE);
            });
        });
    }

    pub fn set_config_colour_from_srgb(&mut self, depth: u8, srgb: [u8; 3], background: bool) {
        let rgb_10b = srgb.map(|c| {
            scale_8b_rgb_to_10b(
                c as u16,
                2.0,
                depth,
                self.state.pattern_config.limited_range,
                false,
            )
        });

        if background {
            self.state.pattern_config.background_colour = rgb_10b;
        } else {
            self.state.pattern_config.patch_colour = rgb_10b;
        }
    }

    fn update_controller_state(&self) {
        self.ctx
            .controller_tx
            .try_send(PGenControllerCmd::UpdateState(self.state.clone()))
            .ok();
    }

    fn update_from_new_state(&mut self, state: PGenControllerState) {
        let prev_depth = self.state.pattern_config.bit_depth;
        let prev_quant_range = QuantRange::from(self.state.pattern_config.limited_range);

        self.state = state;

        // Adjust pattern config for output config
        if let Some(out_cfg) = self.state.pgen_info.as_ref().map(|e| &e.output_config) {
            if out_cfg.quant_range != prev_quant_range {
                scale_pattern_config_rgb_values(
                    &mut self.state.pattern_config,
                    out_cfg.bit_depth as u8,
                    prev_depth as u8,
                    out_cfg.quant_range == QuantRange::Limited,
                    prev_quant_range == QuantRange::Limited,
                );

                self.update_controller_state();
            }
        }
    }

    pub fn compute_max_pattern_size_and_position(&self) -> Option<(u16, u16, u16, u16)> {
        self.state.pgen_info.as_ref().map(|info| {
            let (max_w, max_h) = info.current_display_mode.resolution;
            let (width, height) = self.state.pattern_config.patch_size;
            let (max_pos_x, max_pos_y) = (max_w - width, max_h - height);

            (max_w, max_h, max_pos_x, max_pos_y)
        })
    }
}

fn is_dragvalue_finished(res: egui::Response) -> bool {
    !res.has_focus() && (res.drag_released() || res.lost_focus())
}

fn restart_pgenerator_sw(ctx: &PGenAppContext) {
    ctx.controller_tx
        .try_send(PGenControllerCmd::RestartSoftware)
        .ok();
}

fn send_pgen_set_conf_command(ctx: &PGenAppContext, command: PGenSetConfCommand) {
    let commands = vec![command];
    ctx.controller_tx
        .try_send(PGenControllerCmd::MultipleSetConfCommands(commands))
        .ok();
}

fn update_output_color_format(ctx: &PGenAppContext, format: ColorFormat) {
    let quant_range = match format {
        ColorFormat::Rgb => QuantRange::Full,
        ColorFormat::YCbCr444 | ColorFormat::YCbCr422 => QuantRange::Limited,
    };

    let commands = vec![
        PGenSetConfCommand::SetColorFormat(format),
        PGenSetConfCommand::SetQuantRange(quant_range),
    ];

    ctx.controller_tx
        .try_send(PGenControllerCmd::MultipleSetConfCommands(commands))
        .ok();
}
