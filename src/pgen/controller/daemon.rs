use futures::{stream::StreamExt, FutureExt};
use tokio::{sync::mpsc::Receiver, time::interval};
use tokio_stream::wrappers::ReceiverStream;

use crate::app::PGenAppUpdate;

use super::{handler::PGenController, PGenControllerCmd};

pub fn start_pgen_controller_worker(
    controller: PGenController,
    controller_rx: Receiver<PGenControllerCmd>,
) {
    tokio::spawn(async move {
        init_command_loop(controller, controller_rx).await;
    });
}

async fn init_command_loop(
    mut controller: PGenController,
    controller_rx: Receiver<PGenControllerCmd>,
) {
    let reconnect_period = std::time::Duration::from_secs(900);
    let mut reconnect_stream = interval(reconnect_period);

    let heartbeat_period = std::time::Duration::from_secs(30);
    let mut heartbeat_stream = interval(heartbeat_period);

    let mut rx = ReceiverStream::new(controller_rx).fuse();

    loop {
        futures::select! {
            cmd = rx.select_next_some() => {
                controller.ctx.app_tx.as_ref().and_then(|app_tx| app_tx.try_send(PGenAppUpdate::Processing).ok());

                match cmd {
                    PGenControllerCmd::SetInitialState(state) => controller.set_initial_state(state).await,
                    PGenControllerCmd::UpdateState(state) => controller.state = state,
                    PGenControllerCmd::InitialConnect => controller.initial_connect().await,
                    PGenControllerCmd::UpdateSocket(socket_addr) => controller.update_socket(socket_addr).await,
                    PGenControllerCmd::Disconnect => controller.disconnect().await,
                    PGenControllerCmd::TestPattern(config) => controller.send_pattern_from_cfg(config).await,
                    PGenControllerCmd::SendCurrentPattern => controller.send_current_pattern().await,
                    PGenControllerCmd::SetBlank => controller.set_blank().await,
                    PGenControllerCmd::PGen(cmd) => {
                        controller.pgen_command(cmd).await;
                    },
                    PGenControllerCmd::SetGuiCallback(egui_ctx) => {
                        controller.ctx.egui_ctx.replace(egui_ctx);
                    }
                }

                controller.ctx.app_tx.as_ref().and_then(|app_tx| app_tx.try_send(PGenAppUpdate::DoneProcessing).ok());
                controller.update_ui();
            }
            _ = heartbeat_stream.tick().fuse() => {
                controller.send_heartbeat().await;
            }
            _ = reconnect_stream.tick().fuse() => {
                controller.reconnect().await;
            }
        }
    }
}
