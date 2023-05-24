use std::sync::{Arc, Mutex};

use async_std::channel::{Receiver, Sender};
use eframe::egui;

use super::client::{ConnectState, PGenClient, PGenCommand, PGenCommandResponse};

pub struct PGenController {
    pub processing: bool,
    pub state: ControllerState,

    pub(crate) client: Arc<Mutex<PGenClient>>,

    cmd_sender: Sender<PGenCommandMsg>,
    state_receiver: Receiver<PGenCommandResponse>,

    // For waking up the UI thread
    pub(crate) egui_ctx: Option<egui::Context>,
}

pub struct PGenCommandMsg {
    pub client: Arc<Mutex<PGenClient>>,
    pub cmd: PGenCommand,

    // For waking up the UI thread
    pub egui_ctx: Option<egui::Context>,
}

#[derive(Debug, Default)]
pub struct ControllerState {
    pub connected_state: ConnectState,
}

impl PGenController {
    pub fn new(
        client: PGenClient,
        cmd_sender: Sender<PGenCommandMsg>,
        state_receiver: Receiver<PGenCommandResponse>,
    ) -> Self {
        Self {
            processing: false,
            state: Default::default(),
            client: Arc::new(Mutex::new(client)),
            cmd_sender,
            state_receiver,
            egui_ctx: None,
        }
    }

    pub fn set_egui_context(&mut self, cc: &eframe::CreationContext) {
        self.egui_ctx = Some(cc.egui_ctx.clone());
    }

    pub fn check_responses(&mut self) {
        let has_responses = !self.state_receiver.is_empty();

        while let Ok(res) = self.state_receiver.try_recv() {
            log::trace!("Received PGen command response!");
            println!("{:?}", res);

            match res {
                PGenCommandResponse::NotConnected => self.state.connected_state.connected = false,
                PGenCommandResponse::Alive(is_alive) => {
                    self.state.connected_state.connected = is_alive
                }
                PGenCommandResponse::Busy => (),
                PGenCommandResponse::Connect(state)
                | PGenCommandResponse::Quit(state)
                | PGenCommandResponse::Shutdown(state)
                | PGenCommandResponse::Reboot(state) => self.state.connected_state = state,
            }
        }
        if let Some(egui_ctx) = self.egui_ctx.as_ref() {
            egui_ctx.request_repaint();
        }

        if has_responses {
            self.processing = false;
        }
    }

    pub fn pgen_command(&mut self, cmd: PGenCommand) {
        self.processing = true;

        let msg = PGenCommandMsg {
            client: self.client.clone(),
            cmd,
            egui_ctx: self.egui_ctx.as_ref().cloned(),
        };

        self.cmd_sender.try_send(msg).ok();
    }

    pub async fn reconnect(&mut self) {
        if let Ok(ref mut client) = self.client.lock() {
            // Don't auto connect
            if !client.connect_state.connected {
                return;
            }

            log::trace!("Reconnecting TCP socket stream");
            let res = client.set_stream().await;
            match &res {
                Ok(_) => client.connect_state.connected = true,
                Err(e) => client.connect_state.error = Some(e.to_string()),
            };

            if let Some(egui_ctx) = self.egui_ctx.as_ref() {
                egui_ctx.request_repaint();
            }
        }
    }
}
