use eframe::egui::{self, Key};

use super::PGenApp;

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
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if let Ok(ref mut controller) = self.controller.lock() {
                if ui.input(|i| i.key_pressed(Key::Q) || i.key_pressed(Key::Escape)) {
                    controller.disconnect();
                    self.requested_close = true;
                }
                if self.requested_close
                    && !self.allowed_to_close
                    && !controller.has_messages_queued()
                {
                    // Save before close as we have the lock
                    if let Some(storage) = frame.storage_mut() {
                        eframe::set_value(storage, eframe::APP_KEY, &controller.state);
                    }

                    log::trace!("Nothing queued, closing app");
                    self.allowed_to_close = true;
                }

                self.set_top_bar(ctx, controller);
                self.add_pattern_config(ctx, controller);

                controller.check_responses();
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Don't block here
        if let Ok(controller) = &self.controller.try_lock() {
            eframe::set_value(storage, eframe::APP_KEY, &controller.state);
        }
    }
}
