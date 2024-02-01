use eframe::egui::{self, Key};

use super::{PGenApp, PGenAppSavedState};

impl eframe::App for PGenApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.requested_close && !self.allowed_to_close {
            let close_requested = ctx.input(|i| {
                i.viewport().close_requested()
                    || i.key_pressed(Key::Q)
                    || i.key_pressed(Key::Escape)
            });

            if close_requested {
                self.close();
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        self.set_top_bar(ctx);
        self.set_right_panel(ctx);
        self.set_central_panel(ctx);

        self.check_responses(ctx);

        if self.requested_close && self.allowed_to_close {
            self.requested_close = false;
            log::info!("Cleared queue, closing app");

            if let Some(storage) = frame.storage_mut() {
                save_config(self, storage);
            }

            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        save_config(self, storage);
    }
}

fn save_config(app: &mut PGenApp, storage: &mut dyn eframe::Storage) {
    eframe::set_value(
        storage,
        eframe::APP_KEY,
        &PGenAppSavedState {
            state: app.state.clone(),
            editing_socket: app.editing_socket.clone(),
            generator_type: app.generator_type,
            generator_state: app.generator_state,
            cal_state: app.cal_state.clone(),
        },
    );
}
