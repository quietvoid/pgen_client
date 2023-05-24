use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::pgen::client::PGenCommand;
use crate::pgen::controller::PGenController;

mod eframe_app;

pub(crate) struct PGenApp {
    pub(crate) controller: Arc<Mutex<PGenController>>,
}

impl PGenApp {
    pub fn new(cc: &eframe::CreationContext, controller: Arc<Mutex<PGenController>>) -> Self {
        {
            let controller = controller.clone();
            let mut controller_mutex = controller.lock().unwrap();
            controller_mutex.set_egui_context(cc);
        }

        Self { controller }
    }

    fn set_top_bar(&self, ctx: &egui::Context, controller: &mut PGenController) {
        let connected = controller.state.connected_state.connected;

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            egui::widgets::global_dark_light_mode_switch(ui);

            egui::Grid::new("prefs_grid")
                .num_columns(3)
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
                    ui.add_enabled_ui(!controller.processing, |ui| {
                        if ui.button("Connect").clicked() {
                            controller.pgen_command(PGenCommand::Connect);
                        }

                        if connected && ui.button("Disconnect").clicked() {
                            controller.pgen_command(PGenCommand::Quit);
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

            if controller.processing {
                ui.add(egui::Spinner::new().size(32.0));
            }
        });
    }
}
