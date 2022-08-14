use std::net::IpAddr;
use std::net::SocketAddr;

use async_std::stream::StreamExt;
use async_std::task;
use clap::Parser;

mod app;
mod pgen;

use pgen::{client::PGenClient, controller::PGenController};

#[derive(Parser, Debug)]
#[clap(name = env!("CARGO_PKG_NAME"), about = "RPi PGenerator client", author = "quietvoid", version = env!("CARGO_PKG_VERSION"))]
struct Opt {
    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,

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
    let (sender, mut receiver) = async_std::channel::bounded(5);
    let (state_sender, state_receiver) = async_std::channel::bounded(5);

    let controller = PGenController::new(
        PGenClient::new(SocketAddr::new(opt.ip, opt.port)),
        sender,
        state_receiver,
    );

    // Command loop
    std::thread::spawn(move || {
        task::block_on(async {
            while let Some(msg) = receiver.next().await {
                log::trace!("Channel received PGen command to execute!");

                if let Ok(ref mut client) = msg.client.try_lock() {
                    let res = client.send_generic_command(msg.cmd).await;

                    if state_sender.try_send(res).is_ok() {
                        msg.egui_ctx.request_repaint();
                    }
                } else {
                    log::trace!("Couldn't send command to client, already busy!");
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
