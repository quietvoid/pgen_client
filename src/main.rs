use std::net::IpAddr;
use std::net::SocketAddr;

use async_std::stream::StreamExt;
use async_std::task;
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};

mod app;
mod pgen;

use pgen::{client::PGenClient, controller::PGenController};

#[derive(Parser, Debug)]
#[clap(name = env!("CARGO_PKG_NAME"), about = "RPi PGenerator client", author = "quietvoid", version = env!("CARGO_PKG_VERSION"))]
struct Opt {
    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,

    #[clap(long, short = 'a', help = "IP Address of the PGenerator device")]
    ip: IpAddr,

    #[clap(
        long,
        short = 'p',
        help = "IP Address of the PGenerator device",
        default_value = "85"
    )]
    port: u16,
}

fn main() {
    let opt = Opt::parse();

    pretty_env_logger::formatted_timed_builder()
        .filter_module("pgen_client", opt.verbose.log_level_filter())
        .init();

    let options = eframe::NativeOptions::default();
    let (cmd_sender, mut cmd_receiver) = async_std::channel::bounded(5);
    let (state_sender, state_receiver) = async_std::channel::bounded(5);

    let controller = PGenController::new(
        PGenClient::new(SocketAddr::new(opt.ip, opt.port)),
        cmd_sender,
        state_receiver,
    );

    // Command loop
    std::thread::spawn(move || {
        task::block_on(async {
            while let Some(msg) = cmd_receiver.next().await {
                log::trace!("Channel received PGen command to execute!");

                let res = if let Ok(ref mut client) = msg.client.try_lock() {
                    client.send_generic_command(msg.cmd).await
                } else {
                    log::trace!("Couldn't send command to client, already busy!");
                    pgen::client::PGenCommandResponse::Busy
                };

                if state_sender.try_send(res).is_ok() {
                    msg.egui_ctx.request_repaint();
                }
            }
        });
    });

    eframe::run_native(
        "pgen_client",
        options,
        Box::new(|cc| Box::new(controller.with_cc(cc))),
    );
}
