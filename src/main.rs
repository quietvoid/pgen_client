use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use app::PGenApp;
use async_io::Timer;
use async_std::channel::Receiver;
use async_std::channel::Sender;
use async_std::stream::StreamExt;
use async_std::task;
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use eframe::egui::{self, TextStyle};

mod app;
mod pgen;

use pgen::commands::{PGenCommand, PGenCommandResponse};
use pgen::controller::{PGenCommandMsg, PGenController};

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

    let options = eframe::NativeOptions::default();
    let (cmd_sender, cmd_receiver) = async_std::channel::bounded(5);
    let (state_sender, state_receiver) = async_std::channel::bounded(5);

    let controller = PGenController::new(cmd_sender, state_receiver);
    let controller = Arc::new(Mutex::new(controller));

    {
        let controller = controller.clone();

        // Tasks
        std::thread::spawn(move || {
            init_reconnect_task(controller.clone());
            init_heartbeat_task(controller.clone());
            init_command_loop(cmd_receiver, state_sender);
        });
    }

    let res = eframe::run_native(
        "pgen_client",
        options,
        Box::new(|cc| {
            // Set the global theme, default to dark mode
            let mut global_visuals = egui::style::Visuals::dark();
            global_visuals.window_shadow = egui::epaint::Shadow::small_light();
            cc.egui_ctx.set_visuals(global_visuals);

            let mut style = (*cc.egui_ctx.style()).clone();
            style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 16.0;
            style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 16.0;
            cc.egui_ctx.set_style(style);

            Box::new(PGenApp::new(cc, controller))
        }),
    );

    if let Err(e) = res {
        bail!("Failed starting egui window: {}", e);
    }

    Ok(())
}

fn init_reconnect_task(controller: Arc<Mutex<PGenController>>) {
    task::spawn(async move {
        let reconnect_period = std::time::Duration::from_secs(900);
        while Timer::interval(reconnect_period).next().await.is_some() {
            if let Ok(ref mut controller) = controller.lock() {
                task::block_on(async {
                    controller.reconnect().await;
                });
            }
        }
    });
}

fn init_heartbeat_task(controller: Arc<Mutex<PGenController>>) {
    task::spawn(async move {
        let heartbeat_period = std::time::Duration::from_secs(30);
        while Timer::interval(heartbeat_period).next().await.is_some() {
            if let Ok(ref mut controller) = controller.lock() {
                if controller.state.connected_state.connected {
                    controller.pgen_command(PGenCommand::IsAlive);
                }
            }
        }
    });
}

fn init_command_loop(
    mut cmd_receiver: Receiver<PGenCommandMsg>,
    state_sender: Sender<PGenCommandResponse>,
) {
    task::block_on(async {
        while let Some(msg) = cmd_receiver.next().await {
            log::trace!("Channel received PGen command to execute: {:?}", msg.cmd);

            let res = if let Ok(ref mut client) = msg.client.try_lock() {
                client.send_generic_command(msg.cmd).await
            } else {
                log::trace!("Couldn't send command to client, already busy!");
                PGenCommandResponse::Busy
            };

            if state_sender.try_send(res).is_ok() {
                if let Some(egui_ctx) = msg.egui_ctx {
                    egui_ctx.request_repaint();
                }
            }
        }
    });
}
