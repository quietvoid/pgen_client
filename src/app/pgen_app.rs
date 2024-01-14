use std::net::{IpAddr, SocketAddr};

use eframe::egui::{self, Layout, Sense};
use eframe::epaint::{Color32, Stroke, Vec2};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::generators::{GeneratorClient, GeneratorCmd, GeneratorState};
use crate::pgen::commands::PGenCommand;
use crate::pgen::controller::{PGenControllerCmd, PGenControllerState};
use crate::pgen::pattern_config::{TestPatternPosition, TestPatternSize};
use crate::pgen::utils::{
    compute_rgb_range, rgb_10b_to_8b, scale_8b_rgb_to_10b, scale_rgb_into_range,
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

    pub fn block_until_closed(&mut self) {
        self.requested_close = true;

        let controller_tx = self.ctx.controller_tx.clone();
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                controller_tx.send(PGenControllerCmd::Disconnect).await.ok();

                while let Some(msg) = self.ctx.rx.recv().await {
                    match msg {
                        PGenAppUpdate::NewState(state) => self.state = state,
                        PGenAppUpdate::Processing => self.processing = true,
                        PGenAppUpdate::DoneProcessing => {
                            self.processing = false;
                            self.ctx.rx.close();
                        }
                        _ => {}
                    }
                }
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
                        self.state = saved_state.state;
                        self.editing_socket = saved_state.editing_socket;
                        self.generator_state = saved_state.generator_state;

                        self.ctx
                            .controller_tx
                            .try_send(PGenControllerCmd::SetInitialState(self.state.clone()))
                            .ok();
                    }
                }
                PGenAppUpdate::NewState(state) => self.state = state,
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
        let connected = self.state.connected_state.connected;

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                egui::widgets::global_dark_light_mode_switch(ui);
                if self.processing {
                    ui.add(egui::Spinner::new().size(26.0));
                }
            });

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.label("IP Address");
                let ip_res = ui.add(
                    egui::TextEdit::singleline(&mut self.editing_socket.0).desired_width(255.0),
                );

                ui.label("Port");
                let port_res = ui.add(
                    egui::TextEdit::singleline(&mut self.editing_socket.1).desired_width(50.0),
                );

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

                    ui.add_enabled_ui(!self.processing, |ui| {
                        if ui.button("Connect").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::InitialConnect)
                                .ok();
                        }

                        if connected && ui.button("Disconnect").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::Disconnect)
                                .ok();
                        }

                        if connected && ui.button("Shutdown device").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::PGen(PGenCommand::Shutdown))
                                .ok();
                        }

                        if connected && ui.button("Reboot device").clicked() {
                            self.ctx
                                .controller_tx
                                .try_send(PGenControllerCmd::PGen(PGenCommand::Reboot))
                                .ok();
                        }
                    });
                    ui.end_row();
                });
        });
    }

    pub(crate) fn set_central_panel(&mut self, ctx: &egui::Context) {
        let can_edit_pattern_config = !self.processing && !self.generator_state.listening;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.add_output_info(ui);

            ui.add_enabled_ui(can_edit_pattern_config, |ui| {
                self.add_pattern_config_grid(ui);
            });
            ui.separator();

            ui.add_enabled_ui(!self.processing, |ui| {
                self.add_generator_config(ctx, ui);
            });
        });
    }

    fn add_output_info(&mut self, ui: &mut egui::Ui) {
        let output_config = self
            .state
            .connected_state
            .connected
            .then_some(self.state.output_config.as_ref())
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
    }

    fn add_pattern_config_grid(&mut self, ui: &mut egui::Ui) {
        let connected = self.state.connected_state.connected;
        let old_limited_range = self.state.pattern_config.limited_range;
        let old_depth = self.state.pattern_config.bit_depth;
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

                ui.label("Bit depth");
                egui::ComboBox::from_id_source(egui::Id::new("depth_select"))
                    .width(50.0)
                    .selected_text(self.state.pattern_config.bit_depth.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.state.pattern_config.bit_depth, 8, "8");
                        ui.selectable_value(&mut self.state.pattern_config.bit_depth, 10, "10");
                    });

                ui.end_row();

                let pattern_size_info = connected
                    .then_some(self.compute_max_pattern_size_and_position())
                    .flatten();

                ui.label("Patch size");
                egui::ComboBox::from_id_source(egui::Id::new("preset_size_select"))
                    .selected_text(self.state.pattern_config.preset_size.to_str())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);

                        for preset_size in TestPatternSize::list().iter().copied() {
                            ui.selectable_value(
                                &mut self.state.pattern_config.preset_size,
                                preset_size,
                                preset_size.to_str(),
                            );
                        }
                    });

                if let Some(size_info) = pattern_size_info {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.patch_size.0)
                                .clamp_range(0..=size_info.0),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.patch_size.1)
                                .clamp_range(0..=size_info.1),
                        );
                    });
                }
                ui.end_row();

                ui.label("Position");
                egui::ComboBox::from_id_source(egui::Id::new("preset_position_select"))
                    .selected_text(self.state.pattern_config.preset_position.to_str())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(115.0);
                        for preset_pos in TestPatternPosition::list().iter().copied() {
                            ui.selectable_value(
                                &mut self.state.pattern_config.preset_position,
                                preset_pos,
                                preset_pos.to_str(),
                            );
                        }
                    });

                if let Some(size_info) = pattern_size_info {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.position.0)
                                .clamp_range(0..=size_info.2),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.state.pattern_config.position.1)
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
                        ui.add(egui::DragValue::new(c).clamp_range(rgb_range.clone()));
                    });
                });
                ui.end_row();

                ui.label("Background colour");
                ui.centered_and_justified(|ui| {
                    ui.color_edit_button_srgb(&mut bg_rgb);
                });
                ui.horizontal(|ui| {
                    bg_rgb_dragv.iter_mut().for_each(|c| {
                        ui.add(egui::DragValue::new(c).clamp_range(rgb_range.clone()));
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

        if old_limited_range != self.state.pattern_config.limited_range
            || old_depth != self.state.pattern_config.bit_depth
        {
            self.scale_rgb_values(old_depth, old_limited_range);
            state_updated |= true;
        }
        if old_preset_size != self.state.pattern_config.preset_size
            || old_preset_position != self.state.pattern_config.preset_position
        {
            self.state.set_pattern_size_and_pos_from_resolution();
            state_updated |= true;
        }
        if old_rgb != rgb {
            self.set_config_colour_from_srgb(self.state.pattern_config.bit_depth, rgb, false);
            state_updated |= true;
        }
        if old_bg_rgb != bg_rgb {
            self.set_config_colour_from_srgb(self.state.pattern_config.bit_depth, bg_rgb, true);
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
            self.ctx
                .controller_tx
                .try_send(PGenControllerCmd::UpdateState(self.state.clone()))
                .ok();
        }
    }

    fn add_generator_config(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.with_layout(Layout::left_to_right(egui::Align::Min), |ui| {
            ui.label("Generator client");
            ui.add_enabled_ui(!self.generator_state.listening, |ui| {
                egui::ComboBox::from_id_source(egui::Id::new("generator_client"))
                    .selected_text(self.generator_state.client.to_str())
                    .show_ui(ui, |ui| {
                        for client in GeneratorClient::list().iter().copied() {
                            ui.selectable_value(
                                &mut self.generator_state.client,
                                client,
                                client.to_str(),
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
            });
            let (res, painter) = ui.allocate_painter(Vec2::new(16.0, 16.0), Sense::hover());
            painter.circle(res.rect.center(), 8.0, status_color, Stroke::NONE);
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

    pub fn scale_rgb_values(&mut self, prev_depth: u8, prev_limited_range: bool) {
        let depth = self.state.pattern_config.bit_depth;
        let diff = depth.abs_diff(prev_depth) as f32;

        let limited_range = self.state.pattern_config.limited_range;

        if prev_depth == 8 {
            // 8 bit to 10 bit
            self.state
                .pattern_config
                .patch_colour
                .iter_mut()
                .chain(self.state.pattern_config.background_colour.iter_mut())
                .for_each(|c| {
                    *c = scale_8b_rgb_to_10b(*c, diff, depth, limited_range, prev_limited_range)
                });
        } else {
            // 10 bit to 8 bit
            self.state
                .pattern_config
                .patch_colour
                .iter_mut()
                .chain(self.state.pattern_config.background_colour.iter_mut())
                .for_each(|c| {
                    let mut val = *c as f32 / 2.0_f32.powf(diff);
                    val = scale_rgb_into_range(val, depth, limited_range, prev_limited_range);

                    *c = val.round() as u16;
                });
        }
    }

    pub fn compute_max_pattern_size_and_position(&self) -> Option<(u16, u16, u16, u16)> {
        self.state.output_config.as_ref().map(|out_cfg| {
            let (max_w, max_h) = out_cfg.resolution;
            let (width, height) = self.state.pattern_config.patch_size;
            let (max_pos_x, max_pos_y) = (max_w - width, max_h - height);

            (max_w, max_h, max_pos_x, max_pos_y)
        })
    }
}
