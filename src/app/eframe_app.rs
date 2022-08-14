use eframe::egui::{self, Frame};
use eframe::epaint::Color32;

use crate::pgen::controller::PGenController;

impl PGenController {
    pub fn with_cc(self, cc: &eframe::CreationContext) -> Self {
        // Set the global theme, default to dark mode
        let mut global_visuals = egui::style::Visuals::dark();
        global_visuals.window_shadow = egui::epaint::Shadow::small_light();
        cc.egui_ctx.set_visuals(global_visuals);

        self
    }
}

impl eframe::App for PGenController {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_responses();

        let panel_frame = Frame::default().fill(Color32::from_gray(51));

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.processing, |ui| {
                    if ui.button("Connect").clicked() {
                        self.connect(ctx);
                    }
                });

                if self.processing {
                    ui.add(egui::Spinner::new().size(32.0));
                }
            });
    }
}
