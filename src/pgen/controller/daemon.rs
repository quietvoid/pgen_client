use futures::{FutureExt, stream::StreamExt};
use tokio::{sync::mpsc::Receiver, time::interval};
use tokio_stream::wrappers::ReceiverStream;

use crate::app::PGenAppUpdate;

use super::{PGenControllerCmd, PGenControllerHandle};

pub fn start_pgen_controller_worker(
    controller: PGenControllerHandle,
    controller_rx: Receiver<PGenControllerCmd>,
) {
    tokio::spawn(async move {
        init_command_loop(controller, controller_rx).await;
    });
}

async fn init_command_loop(
    controller_handle: PGenControllerHandle,
    controller_rx: Receiver<PGenControllerCmd>,
) {
    let reconnect_period = std::time::Duration::from_secs(30 * 60);
    let mut reconnect_stream = interval(reconnect_period);

    let heartbeat_period = std::time::Duration::from_secs(30);
    let mut heartbeat_stream = interval(heartbeat_period);

    let mut rx = ReceiverStream::new(controller_rx).fuse();

    loop {
        futures::select! {
            cmd = rx.select_next_some() => {
                let mut controller = controller_handle.lock().await;
                controller.ctx.app_tx.as_ref().and_then(|app_tx| app_tx.try_send(PGenAppUpdate::Processing).ok());

                match cmd {
                    PGenControllerCmd::SetGuiCallback(egui_ctx) => {
                        controller.ctx.egui_ctx.replace(egui_ctx);
                    }
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
                    PGenControllerCmd::RestartSoftware => controller.restart_pgenerator_software(true).await,
                    PGenControllerCmd::ChangeDisplayMode(mode) => controller.change_display_mode(mode, true).await,
                    PGenControllerCmd::UpdateDynamicRange(dynamic_range) => controller.update_dynamic_range(dynamic_range).await,
                    PGenControllerCmd::MultipleSetConfCommands(commands) => {
                        // Restart must be done manually to apply changes
                        controller.send_multiple_set_conf_commands(commands).await
                    },
                }

                controller.ctx.app_tx.as_ref().and_then(|app_tx| app_tx.try_send(PGenAppUpdate::DoneProcessing).ok());
                controller.update_ui();
            }
            _ = heartbeat_stream.tick().fuse() => {
                let mut controller = controller_handle.lock().await;
                controller.send_heartbeat().await;
            }
            _ = reconnect_stream.tick().fuse() => {
                let mut controller = controller_handle.lock().await;
                controller.reconnect().await;
            }
        }
    }
}
