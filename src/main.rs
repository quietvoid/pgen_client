use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use app::{commands::PGenAppContext, PGenApp};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use eframe::egui::{self, TextStyle};
use pgen::controller::PGenController;

mod app;
mod pgen;

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), about = "RPi PGenerator client", author = "quietvoid", version = env!("CARGO_PKG_VERSION"))]
struct Opt {
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn main() -> Result<()> {
    let opt = Opt::parse();

    pretty_env_logger::formatted_timed_builder()
        .filter_module("pgen_client", opt.verbose.log_level_filter())
        .init();

    let (app_sender, app_receiver) = async_std::channel::bounded(5);
    let (res_sender, res_receiver) = async_std::channel::bounded(5);

    let controller = PGenController::new(app_sender.clone());
    let controller = Arc::new(RwLock::new(controller));

    let app_ctx = PGenAppContext {
        app_sender,
        res_sender,
        res_receiver,
        controller,
    };
    pgen::daemon::start_pgen_daemon_thread(app_ctx.clone(), app_receiver);

    let res = eframe::run_native(
        "pgen_client",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            // Set the global theme, default to dark mode
            let mut global_visuals = egui::style::Visuals::dark();
            global_visuals.window_shadow = egui::epaint::Shadow::small_light();
            cc.egui_ctx.set_visuals(global_visuals);

            let mut style = (*cc.egui_ctx.style()).clone();
            style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 16.0;
            style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 16.0;
            cc.egui_ctx.set_style(style);

            Box::new(PGenApp::new(cc, app_ctx))
        }),
    );

    if let Err(e) = res {
        bail!("Failed starting egui window: {}", e);
    }

    Ok(())
}
