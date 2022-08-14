use std::sync::{Arc, Mutex};

use async_std::channel::{Receiver, Sender};
use eframe::egui;

use super::client::{ConnectState, PGenClient, PGenCommand, PGenCommandResponse};

pub struct PGenController {
    pub processing: bool,
    pub state: ControllerState,

    client: Arc<Mutex<PGenClient>>,

    cmd_sender: Sender<PGenCommandMsg>,
    state_receiver: Receiver<PGenCommandResponse>,
}

pub struct PGenCommandMsg {
    // For waking up the UI thread
    pub egui_ctx: egui::Context,

    pub client: Arc<Mutex<PGenClient>>,
    pub cmd: PGenCommand,
}

#[derive(Debug, Default)]
pub struct ControllerState {
    connected_state: ConnectState,
}

impl PGenController {
    pub fn new(
        client: PGenClient,
        cmd_sender: Sender<PGenCommandMsg>,
        state_receiver: Receiver<PGenCommandResponse>,
    ) -> Self {
        Self {
            processing: false,
            client: Arc::new(Mutex::new(client)),
            cmd_sender,
            state_receiver,
            state: Default::default(),
        }
    }

    pub fn check_responses(&mut self) {
        while let Ok(res) = self.state_receiver.try_recv() {
            log::trace!("Received PGen command response!");
            println!("{:?}", res);

            match res {
                PGenCommandResponse::Connect(state) => self.state.connected_state = state,
            }
        }
    }

    pub fn connect(&mut self, ctx: &egui::Context) {
        self.processing = true;

        let msg = PGenCommandMsg {
            egui_ctx: ctx.clone(),
            client: self.client.clone(),
            cmd: PGenCommand::Connect,
        };

        self.cmd_sender.try_send(msg).ok();
    }
}
