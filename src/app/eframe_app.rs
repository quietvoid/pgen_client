use eframe::egui::{self, Key};

use crate::{generators::GeneratorState, pgen::controller::PGenControllerCmd};

use super::{PGenApp, PGenAppSavedState};

impl eframe::App for PGenApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ctx.input(|i| i.viewport().close_requested()) && !self.allowed_to_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }

            if ui.input(|i| i.key_pressed(Key::Q) || i.key_pressed(Key::Escape)) {
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

            self.set_top_bar(ctx);
            self.set_central_panel(ctx);

            self.check_responses();

            if self.requested_close && self.allowed_to_close {
                self.requested_close = false;
                log::info!("Cleared queue, closing app");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            eframe::APP_KEY,
            &PGenAppSavedState {
                state: self.state.clone(),
                editing_socket: self.editing_socket.clone(),
                generator_state: GeneratorState {
                    client: self.generator_state.client,
                    listening: false,
                },
            },
        );
    }
}
