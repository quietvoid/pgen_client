use eframe::egui;

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
    fn clear_color(&self, visuals: &egui::Visuals) -> egui::Rgba {
        visuals.window_fill().into()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_responses();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            egui::widgets::global_dark_light_mode_switch(ui);

            egui::Grid::new("prefs_grid")
                .num_columns(3)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Status");

                    let status_str = if self.state.connected_state.connected {
                        "Connected"
                    } else if let Some(err) = &self.state.connected_state.connect_error {
                        err.as_str()
                    } else {
                        "Not connected"
                    };

                    ui.label(status_str);
                    ui.add_enabled_ui(!self.processing, |ui| {
                        if ui.button("Connect").clicked() {
                            self.connect(ctx);
                        }
                    });
                });

            if self.processing {
                ui.add(egui::Spinner::new().size(32.0));
            }
        });
    }
}
