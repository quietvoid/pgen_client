use eframe::{
    egui::{self, Context, Sense, Stroke, Ui},
    epaint::Vec2,
};
use strum::IntoEnumIterator;

use crate::{external::ExternalJobCmd, generators::GeneratorClient};

use super::{PGenApp, status_color_active};

pub fn add_external_generator_ui(app: &mut PGenApp, ctx: &Context, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("Generator client");
        ui.add_enabled_ui(!app.generator_state.listening, |ui| {
            egui::ComboBox::from_id_salt(egui::Id::new("generator_client"))
                .selected_text(app.generator_state.client.as_ref())
                .show_ui(ui, |ui| {
                    for client in GeneratorClient::iter() {
                        ui.selectable_value(
                            &mut app.generator_state.client,
                            client,
                            client.as_ref(),
                        );
                    }
                });
        });

        let generator_label = if app.generator_state.listening {
            "Stop generator client"
        } else {
            "Start generator client"
        };
        let status_color = status_color_active(ctx, app.generator_state.listening);
        ui.add_enabled_ui(app.state.connected_state.connected, |ui| {
            if ui.button(generator_label).clicked() {
                let cmd = if app.generator_state.listening {
                    ExternalJobCmd::StopGeneratorClient(app.generator_state.client)
                } else {
                    ExternalJobCmd::StartGeneratorClient(app.generator_state.client)
                };

                app.ctx.external_tx.try_send(cmd).ok();
            }

            let (res, painter) = ui.allocate_painter(Vec2::new(16.0, 16.0), Sense::hover());
            painter.circle(res.rect.center(), 8.0, status_color, Stroke::NONE);
        });
    });
}
