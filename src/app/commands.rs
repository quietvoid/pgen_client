use async_std::channel::{Receiver, Sender};

use crate::pgen::{
    commands::PGenCommandResponse,
    controller::{PGenCommandMsg, PGenControllerHandle},
    interfaces::GeneratorInterface,
};

#[derive(Debug, Clone)]
pub(crate) struct PGenAppContext {
    pub(crate) app_sender: Sender<AppCommandTx>,
    pub(crate) res_sender: Sender<AppCommandRx>,
    pub(crate) res_receiver: Receiver<AppCommandRx>,

    pub(crate) controller: PGenControllerHandle,
}

pub enum AppCommandTx {
    Quit,
    StartInterface(GeneratorInterface),
    StopInterface(GeneratorInterface),
    Pgen(PGenCommandMsg),
}

pub enum AppCommandRx {
    GeneratorListening(bool),
    Pgen(PGenCommandResponse),
}
