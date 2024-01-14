use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    generators::{GeneratorCmd, GeneratorState},
    pgen::controller::{PGenControllerCmd, PGenControllerState},
};

pub mod eframe_app;
pub mod pgen_app;

pub use pgen_app::PGenApp;

#[derive(Debug)]
pub struct PGenAppContext {
    pub rx: Receiver<PGenAppUpdate>,

    pub controller_tx: Sender<PGenControllerCmd>,
    pub generator_tx: Sender<GeneratorCmd>,
}

#[derive(Debug, Clone)]
pub enum PGenAppUpdate {
    GeneratorListening(bool),
    InitialSetup {
        egui_ctx: eframe::egui::Context,
        saved_state: Option<PGenAppSavedState>,
    },
    NewState(PGenControllerState),
    Processing,
    DoneProcessing,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PGenAppSavedState {
    pub state: PGenControllerState,
    pub editing_socket: (String, String),
    pub generator_state: GeneratorState,
}
