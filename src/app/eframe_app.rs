use eframe::egui::{self, Key};

use super::{PGenApp, PGenAppSavedState};

impl eframe::App for PGenApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allowed_to_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        if ctx.input(|i| i.key_pressed(Key::Q) || i.key_pressed(Key::Escape)) {
            self.close();
        }

        self.set_top_bar(ctx);
        self.set_right_panel(ctx);
        self.set_central_panel(ctx);

        self.check_responses();

        if self.requested_close && self.allowed_to_close {
            self.requested_close = false;
            log::info!("Cleared queue, closing app");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            eframe::APP_KEY,
            &PGenAppSavedState {
                state: self.state.clone(),
                editing_socket: self.editing_socket.clone(),
                generator_type: self.generator_type,
                generator_state: self.generator_state,
                cal_state: self.cal_state.clone(),
            },
        );
    }
}
