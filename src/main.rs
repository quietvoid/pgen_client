use std::sync::Arc;

use anyhow::{bail, Result};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use eframe::egui::{self, TextStyle};
use tokio::sync::Mutex;

use app::{PGenApp, PGenAppSavedState, PGenAppUpdate};
use pgen::controller::handler::PGenController;

pub mod app;
pub mod calibration;
pub mod external;
pub mod generators;
pub mod pgen;
pub mod spotread;
pub mod utils;

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), about = "RPi PGenerator client", author = "quietvoid", version = env!("CARGO_PKG_VERSION"))]
struct Opt {
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<()> {
    let opt = Opt::parse();

    pretty_env_logger::formatted_timed_builder()
        .filter_module("pgen_client", opt.verbose.log_level_filter())
        .init();

    let (app_tx, app_rx) = tokio::sync::mpsc::channel(5);
    let (controller_tx, controller_rx) = tokio::sync::mpsc::channel(5);
    let controller = Arc::new(Mutex::new(PGenController::new(Some(app_tx.clone()))));

    let external_tx = external::start_external_jobs_worker(
        app_tx.clone(),
        controller_tx.clone(),
        controller.clone(),
    );

    let app = PGenApp::new(app_rx, controller_tx, external_tx);

    pgen::controller::daemon::start_pgen_controller_worker(controller, controller_rx);

    let res = eframe::run_native(
        "pgen_client",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            // Set the global theme, default to dark mode
            let mut global_visuals = egui::style::Visuals::dark();
            global_visuals.window_shadow = egui::epaint::Shadow::small_light();
            cc.egui_ctx.set_visuals(global_visuals);

            let mut style = (*cc.egui_ctx.style()).clone();
            style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 16.0;
            style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 16.0;
            cc.egui_ctx.set_style(style);

            let saved_state = cc.storage.and_then(|storage| {
                eframe::get_value::<PGenAppSavedState>(storage, eframe::APP_KEY)
            });

            app_tx
                .try_send(PGenAppUpdate::InitialSetup {
                    egui_ctx: cc.egui_ctx.clone(),
                    saved_state,
                })
                .ok();

            Box::new(app)
        }),
    );

    if let Err(e) = res {
        bail!("Failed starting egui window: {}", e);
    }

    Ok(())
}
