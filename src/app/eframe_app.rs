use eframe::egui;

use super::PGenApp;

impl eframe::App for PGenApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(ref mut controller) = self.controller.lock() {
            controller.check_responses();

            self.set_top_bar(ctx, controller);
        }
    }
}
