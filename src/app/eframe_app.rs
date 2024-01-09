use eframe::egui::{self, Key};
use serde::{Deserialize, Serialize};

use crate::pgen::{controller::ControllerState, interfaces::GeneratorInfo};

use super::{commands::AppCommandTx, PGenApp};

#[derive(Deserialize, Serialize)]
pub struct PGenAppSavedState {
    pub generator_info: GeneratorInfo,
    pub controller_state: ControllerState,
}

impl eframe::App for PGenApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ctx.input(|i| i.viewport().close_requested()) && !self.allowed_to_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if self.allowed_to_close {
                self.ctx.app_sender.try_send(AppCommandTx::Quit).ok();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if let Ok(ref mut controller) = self.ctx.controller.write() {
                if ui.input(|i| i.key_pressed(Key::Q) || i.key_pressed(Key::Escape)) {
                    controller.disconnect();
                    self.requested_close = true;
                }
                if self.requested_close && !self.allowed_to_close && !self.has_messages_queued() {
                    log::trace!("Nothing queued, closing app");
                    // Save before close as we have the lock
                    if let Some(storage) = frame.storage_mut() {
                        eframe::set_value(storage, eframe::APP_KEY, &controller.state);
                    }

                    self.allowed_to_close = true;
                }

                self.set_top_bar(ctx, controller);
                self.set_central_panel(ctx, &self.ctx, controller);
            }

            self.check_responses();
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(controller) = &self.ctx.controller.read() {
            eframe::set_value(
                storage,
                eframe::APP_KEY,
                &PGenAppSavedState {
                    generator_info: self.generator_info,
                    controller_state: controller.state.clone(),
                },
            );
        }
    }
}
