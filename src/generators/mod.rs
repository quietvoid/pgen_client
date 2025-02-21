use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIs, EnumIter};

pub mod internal;
pub mod resolve;
pub mod tcp_generator_client;

pub use tcp_generator_client::{
    TcpGeneratorClient, TcpGeneratorInterface, start_tcp_generator_client,
};

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub struct GeneratorState {
    pub client: GeneratorClient,
    pub listening: bool,
}

#[derive(
    Display,
    AsRefStr,
    Debug,
    Default,
    Deserialize,
    Serialize,
    Copy,
    Clone,
    PartialEq,
    Eq,
    EnumIter,
    EnumIs,
)]
pub enum GeneratorType {
    #[default]
    Internal,
    External,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub enum GeneratorInterface {
    Tcp(TcpGeneratorInterface),
}

#[derive(
    Display, AsRefStr, Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, EnumIter,
)]
pub enum GeneratorClient {
    #[default]
    Resolve,
}

#[derive(Debug, Clone)]
pub enum GeneratorClientCmd {
    Shutdown,
}

impl GeneratorState {
    pub fn initial_setup(&mut self) {
        self.listening = false;
    }
}

impl GeneratorClient {
    pub const fn interface(&self) -> GeneratorInterface {
        match self {
            Self::Resolve => GeneratorInterface::Tcp(TcpGeneratorInterface::Resolve),
        }
    }
}

impl GeneratorInterface {
    pub const fn client(&self) -> GeneratorClient {
        match self {
            GeneratorInterface::Tcp(tcp_interface) => match tcp_interface {
                TcpGeneratorInterface::Resolve => GeneratorClient::Resolve,
            },
        }
    }
}
