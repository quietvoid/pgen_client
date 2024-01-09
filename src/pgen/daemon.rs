use std::{net::Shutdown, sync::Arc};

use async_io::Timer;
use async_std::{channel::Receiver, sync::Mutex, task};
use futures::{pin_mut, StreamExt};

use crate::{
    app::commands::{AppCommandRx, AppCommandTx, PGenAppContext},
    pgen::interfaces::{
        resolve::{handle_resolve_pattern_message, resolve_connect_and_set_tcp_stream},
        GeneratorInterface,
    },
};

use super::{
    commands::{PGenCommand, PGenCommandResponse},
    interfaces::TcpGeneratorClient,
};

pub fn start_pgen_daemon_thread(app_ctx: PGenAppContext, app_receiver: Receiver<AppCommandTx>) {
    std::thread::spawn(|| task::block_on(run_command_loop(app_ctx, app_receiver)));
}

async fn run_command_loop(app_ctx: PGenAppContext, mut app_receiver: Receiver<AppCommandTx>) {
    let reconnect_period = std::time::Duration::from_secs(900);
    let mut reconnect_stream = Timer::interval(reconnect_period).fuse();

    let heartbeat_period = std::time::Duration::from_secs(30);
    let mut heartbeat_stream = Timer::interval(heartbeat_period).fuse();

    let generator_client = TcpGeneratorClient {
        stream: None,
        interface: GeneratorInterface::Resolve,
        buf: vec![0; 512],
    };
    let generator_client = Arc::new(Mutex::new(generator_client));

    let stream = TcpGeneratorClient::get_stream(generator_client.clone()).fuse();
    pin_mut!(stream);

    loop {
        futures::select_biased! {
            msg = app_receiver.select_next_some() => {
                match msg {
                    AppCommandTx::Quit => {
                        log::trace!("Received quit command!");
                        break;
                    },
                    AppCommandTx::StartInterface(interface) => {
                        log::trace!("Starting interface {interface:?}");

                        match interface {
                            GeneratorInterface::Resolve => {
                                if resolve_connect_and_set_tcp_stream(generator_client.clone()).await {
                                    app_ctx.res_sender.try_send(AppCommandRx::GeneratorListening(true)).ok();
                                }
                            }
                        };
                    },
                    AppCommandTx::StopInterface(interface) => {
                        log::trace!("Stopping interface {interface:?}");

                        let mut client = generator_client.lock().await;
                        if let Some(reader) = client.stream.take() {
                            let stream = reader.into_inner();
                            stream.shutdown(Shutdown::Read).unwrap();
                        }

                        app_ctx.res_sender.try_send(AppCommandRx::GeneratorListening(false)).ok();
                    }
                    AppCommandTx::Pgen(msg) => {
                        log::trace!("Channel received PGen command to execute: {:?}", msg.cmd);

                        let res = if let Ok(ref mut client) = msg.client.try_lock() {
                            client.send_generic_command(msg.cmd).await
                        } else {
                            log::trace!("Couldn't send command to client, already busy!");
                            PGenCommandResponse::Busy
                        };

                        if app_ctx.res_sender.try_send(AppCommandRx::Pgen(res)).is_ok() {
                            if let Some(egui_ctx) = msg.egui_ctx {
                                egui_ctx.request_repaint();
                            }
                        }
                    },
                }
            }

            msg = stream.select_next_some() => {
                let interface = {
                    let client = generator_client.lock().await;
                    client.interface
                };

                match interface {
                    GeneratorInterface::Resolve => handle_resolve_pattern_message(app_ctx.controller.clone(), &msg),
                };
            }

            _ = heartbeat_stream.select_next_some() => {
                if let Ok(ref mut controller) = app_ctx.controller.read() {
                    if controller.state.connected_state.connected {
                        controller.pgen_command(PGenCommand::IsAlive);
                    }
                }
            }
            _ = reconnect_stream.select_next_some() => {
                if let Ok(ref mut controller) = app_ctx.controller.write() {
                    controller.reconnect().await;
                }
            }
        }
    }
}
