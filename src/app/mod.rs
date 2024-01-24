use eframe::{
    egui,
    epaint::{Color32, ColorImage},
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    external::ExternalJobCmd,
    generators::{GeneratorState, GeneratorType},
    pgen::controller::{PGenControllerCmd, PGenControllerState},
    spotread::ReadingResult,
};

mod calibration;
pub mod eframe_app;
mod external_generator_ui;
mod internal_generator_ui;
pub mod pgen_app;

pub use pgen_app::PGenApp;

pub use calibration::{compute_cie_chromaticity_diagram_worker, CalibrationState};

#[derive(Debug)]
pub struct PGenAppContext {
    pub rx: Receiver<PGenAppUpdate>,

    pub controller_tx: Sender<PGenControllerCmd>,
    pub external_tx: Sender<ExternalJobCmd>,
}

pub enum PGenAppUpdate {
    GeneratorListening(bool),
    InitialSetup {
        egui_ctx: eframe::egui::Context,
        saved_state: Box<Option<PGenAppSavedState>>,
    },
    NewState(PGenControllerState),
    Processing,
    DoneProcessing,
    SpotreadStarted(bool),
    SpotreadRes(Option<ReadingResult>),
    CieDiagramReady(ColorImage),
}

#[derive(Deserialize, Serialize)]
pub struct PGenAppSavedState {
    pub state: PGenControllerState,
    pub editing_socket: (String, String),
    pub generator_type: GeneratorType,
    pub generator_state: GeneratorState,
    pub cal_state: CalibrationState,
}

fn status_color_active(ctx: &egui::Context, active: bool) -> Color32 {
    let dark_mode = ctx.style().visuals.dark_mode;
    if active {
        if dark_mode {
            Color32::DARK_GREEN
        } else {
            Color32::LIGHT_GREEN
        }
    } else if dark_mode {
        Color32::DARK_RED
    } else {
        Color32::LIGHT_RED
    }
}
