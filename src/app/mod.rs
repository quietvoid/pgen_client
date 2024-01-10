use std::net::{IpAddr, SocketAddr};

use eframe::egui::{self, Layout, Sense};
use eframe::epaint::{Color32, Stroke, Vec2};

use crate::pgen::commands::PGenCommand;
use crate::pgen::controller::PGenController;
use crate::pgen::interfaces::GeneratorInfo;
use crate::pgen::pattern_config::{TestPatternPosition, TestPatternSize};
use crate::pgen::{compute_rgb_range, rgb_10b_to_8b};

use self::commands::{AppCommandRx, AppCommandTx, PGenAppContext};
use self::eframe_app::PGenAppSavedState;

pub(crate) mod commands;
mod eframe_app;

pub(crate) struct PGenApp {
    pub(crate) ctx: PGenAppContext,
    generator_info: GeneratorInfo,

    requested_close: bool,
    allowed_to_close: bool,
}

impl PGenApp {
    pub fn new(cc: &eframe::CreationContext, app_ctx: PGenAppContext) -> Self {
        let controller = app_ctx.controller.clone();

        let mut app = Self {
            ctx: app_ctx,
            generator_info: Default::default(),
            requested_close: false,
            allowed_to_close: false,
        };

        let mut controller = controller.write().unwrap();

        controller.set_egui_context(cc);

        // Load existing or default state
        if let Some(storage) = cc.storage {
            if let Some(saved_state) =
                eframe::get_value::<PGenAppSavedState>(storage, eframe::APP_KEY)
            {
                app.generator_info = saved_state.generator_info;
                controller.restore_state(saved_state.controller_state);

                if let Ok(ref mut client) = controller.client.lock() {
                    client.set_socket_address(&controller.state.socket_addr);
                }
            }
        }

        app.generator_info.listening = false;

        app
    }

    pub fn has_messages_queued(&self) -> bool {
        !self.ctx.app_sender.is_empty() || !self.ctx.res_receiver.is_empty()
    }

    pub fn processing(&self) -> bool {
        self.has_messages_queued()
    }

    fn check_responses(&mut self) {
        if let Ok(ref mut controller) = self.ctx.controller.write() {
            while let Ok(msg) = self.ctx.res_receiver.try_recv() {
                match msg {
                    AppCommandRx::GeneratorListening(v) => {
                        log::trace!("Generator listening: {v}");
                        self.generator_info.listening = v
                    }
                    AppCommandRx::Pgen(res) => {
                        controller.handle_pgen_response(res);
                    }
                }
            }

            if let Some(egui_ctx) = controller.egui_ctx.as_ref() {
                egui_ctx.request_repaint();
            }
        }
    }

    fn set_top_bar(&self, ctx: &egui::Context, controller: &mut PGenController) {
        let connected = controller.state.connected_state.connected;
        let processing = self.processing();

        let mut socket_changed = false;

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                egui::widgets::global_dark_light_mode_switch(ui);
                if processing {
                    ui.add(egui::Spinner::new().size(26.0));
                }
            });

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.label("IP Address");
                let ip_res = ui.add(
                    egui::TextEdit::singleline(&mut controller.state.editing_socket.0)
                        .desired_width(255.0),
                );

                ui.label("Port");
                let port_res = ui.add(
                    egui::TextEdit::singleline(&mut controller.state.editing_socket.1)
                        .desired_width(50.0),
                );

                socket_changed = ip_res.lost_focus() || port_res.lost_focus();
            });

            egui::Grid::new("prefs_grid")
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Status");

                    let status_str = if connected {
                        "Connected"
                    } else if let Some(err) = &controller.state.connected_state.error {
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

                    ui.add_enabled_ui(!processing, |ui| {
                        if ui.button("Connect").clicked() {
                            controller.initial_connect();
                        }

                        if connected && ui.button("Disconnect").clicked() {
                            controller.disconnect();
                        }

                        if connected && ui.button("Shutdown device").clicked() {
                            controller.pgen_command(PGenCommand::Shutdown);
                        }

                        if connected && ui.button("Reboot device").clicked() {
                            controller.pgen_command(PGenCommand::Reboot);
                        }
                    });
                    ui.end_row();
                });
        });

        if socket_changed {
            let parsed_ip = controller.state.editing_socket.0.parse::<IpAddr>();
            let parsed_port = controller.state.editing_socket.1.parse::<u16>();

            if let (Ok(new_ip), Ok(new_port)) = (&parsed_ip, &parsed_port) {
                let new_socket: SocketAddr = SocketAddr::new(*new_ip, *new_port);
                if controller.state.socket_addr != new_socket {
                    controller.state.socket_addr = new_socket;
                    controller
                        .pgen_command(PGenCommand::UpdateSocket(controller.state.socket_addr));
                }
            } else {
                // Clear invalid back to current socket
                if parsed_ip.is_err() {
                    controller.state.editing_socket.0 =
                        controller.state.socket_addr.ip().to_string();
                }
                if parsed_port.is_err() {
                    controller.state.editing_socket.1 =
                        controller.state.socket_addr.port().to_string();
                }
            }
        }
    }

    fn set_central_panel(
        &self,
        ctx: &egui::Context,
        app_ctx: &PGenAppContext,
        controller: &mut PGenController,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.add_pattern_config(ui, controller);
            ui.separator();

            self.add_generator_config(ctx, app_ctx, ui);
        });
    }

    fn add_pattern_config(&self, ui: &mut egui::Ui, controller: &mut PGenController) {
        let connected = controller.state.connected_state.connected;

        let old_preset_size = controller.state.pattern_config.preset_size;
        let old_preset_position = controller.state.pattern_config.preset_position;

        let output_config = connected
            .then_some(controller.state.output_config.as_ref())
            .flatten();

        if let Some(output_cfg) = output_config {
            egui::Grid::new("output_conf_grid").show(ui, |ui| {
                let (res_w, res_h) = output_cfg.resolution;
                let dynamic_range_str = output_cfg.dynamic_range.to_str();
                let pixel_range = if output_cfg.limited_range {
                    "Limited"
                } else {
                    "Full"
                };

                ui.label(format!("Resolution: {res_w}x{res_h}"));
                ui.end_row();

                ui.label(format!(
                    "Format {} ({pixel_range})",
                    output_cfg.format.to_str()
                ));
                ui.end_row();

                ui.label(format!("Dynamic Range: {dynamic_range_str}"));
                ui.end_row();
            });

            ui.separator();
        }

        if output_config.is_some() {
            ui.separator();
        }

        ui.add_enabled_ui(!self.generator_info.listening, |ui| {
            self.add_pattern_config_grid(controller, ui);
        });

        if old_preset_size != controller.state.pattern_config.preset_size
            || old_preset_position != controller.state.pattern_config.preset_position
        {
            controller.set_pattern_size_and_pos_from_resolution();
        }
    }

    fn add_pattern_config_grid(&self, controller: &mut PGenController, ui: &mut egui::Ui) {
        let connected = controller.state.connected_state.connected;
        let old_depth = controller.state.pattern_config.bit_depth;
        let old_limited_range = controller.state.pattern_config.limited_range;
        let rgb_range = compute_rgb_range(old_limited_range, old_depth);

        let old_rgb = rgb_10b_to_8b(controller.state.pattern_config.patch_colour);
        let mut rgb = old_rgb;

        let old_bg_rgb = rgb_10b_to_8b(controller.state.pattern_config.background_colour);
        let mut bg_rgb = old_bg_rgb;

        egui::Grid::new("pattern_conf_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                ui.label("Limited range");
                ui.add(egui::Checkbox::without_text(
                    &mut controller.state.pattern_config.limited_range,
                ));
                ui.end_row();

                ui.label("Bit depth");
                egui::ComboBox::from_id_source(egui::Id::new("depth_select"))
                    .width(50.0)
                    .selected_text(controller.state.pattern_config.bit_depth.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut controller.state.pattern_config.bit_depth, 8, "8");
                        ui.selectable_value(
                            &mut controller.state.pattern_config.bit_depth,
                            10,
                            "10",
                        );
                    });

                ui.end_row();

                let pattern_size_info = connected
                    .then_some(controller.compute_max_pattern_size_and_position())
                    .flatten();

                ui.label("Patch size");
                egui::ComboBox::from_id_source(egui::Id::new("preset_size_select"))
                    .selected_text(controller.state.pattern_config.preset_size.to_str())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);

                        for preset_size in TestPatternSize::list().iter().copied() {
                            ui.selectable_value(
                                &mut controller.state.pattern_config.preset_size,
                                preset_size,
                                preset_size.to_str(),
                            );
                        }
                    });

                if let Some(size_info) = pattern_size_info {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut controller.state.pattern_config.patch_size.0)
                                .clamp_range(0..=size_info.0),
                        );
                        ui.add(
                            egui::DragValue::new(&mut controller.state.pattern_config.patch_size.1)
                                .clamp_range(0..=size_info.1),
                        );
                    });
                }
                ui.end_row();

                ui.label("Position");
                egui::ComboBox::from_id_source(egui::Id::new("preset_position_select"))
                    .selected_text(controller.state.pattern_config.preset_position.to_str())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);
                        for preset_pos in TestPatternPosition::list().iter().copied() {
                            ui.selectable_value(
                                &mut controller.state.pattern_config.preset_position,
                                preset_pos,
                                preset_pos.to_str(),
                            );
                        }
                    });

                if let Some(size_info) = pattern_size_info {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut controller.state.pattern_config.position.0)
                                .clamp_range(0..=size_info.2),
                        );
                        ui.add(
                            egui::DragValue::new(&mut controller.state.pattern_config.position.1)
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
                    controller
                        .state
                        .pattern_config
                        .patch_colour
                        .iter_mut()
                        .for_each(|c| {
                            ui.add(egui::DragValue::new(c).clamp_range(rgb_range.clone()));
                        });
                });
                ui.end_row();

                ui.label("Background colour");
                ui.centered_and_justified(|ui| {
                    ui.color_edit_button_srgb(&mut bg_rgb);
                });
                ui.horizontal(|ui| {
                    controller
                        .state
                        .pattern_config
                        .background_colour
                        .iter_mut()
                        .for_each(|c| {
                            ui.add(egui::DragValue::new(c).clamp_range(rgb_range.clone()));
                        });
                });
                ui.end_row();

                ui.add_enabled_ui(connected, |ui| {
                    if ui.button("Send pattern").clicked() {
                        controller.send_current_pattern();
                    }

                    if ui.button("Blank pattern").clicked() {
                        controller.set_blank();
                    }
                });
                ui.end_row();
            });

        if old_rgb != rgb {
            controller.set_config_colour_from_srgb(rgb, false);
        }
        if old_bg_rgb != bg_rgb {
            controller.set_config_colour_from_srgb(bg_rgb, true);
        }

        if old_depth != controller.state.pattern_config.bit_depth
            || old_limited_range != controller.state.pattern_config.limited_range
        {
            controller.scale_rgb_values(old_depth, old_limited_range);
        }
    }

    fn add_generator_config(
        &self,
        ctx: &egui::Context,
        app_ctx: &PGenAppContext,
        ui: &mut egui::Ui,
    ) {
        ui.with_layout(Layout::left_to_right(egui::Align::Min), |ui| {
            let generator_label = if self.generator_info.listening {
                "Stop generator interface"
            } else {
                "Start generator interface"
            };

            if ui.button(generator_label).clicked() {
                let cmd = if self.generator_info.listening {
                    AppCommandTx::StopInterface(self.generator_info.interface)
                } else {
                    AppCommandTx::StartInterface(self.generator_info.interface)
                };

                app_ctx.app_sender.try_send(cmd).ok();
            }
            let status_color = if self.generator_info.listening {
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
        });
    }
}
