use std::net::SocketAddr;

use itertools::Itertools;

use super::client::{ConnectState, PGenTestPattern};

#[derive(Debug)]
pub enum PGenCommand {
    IsAlive,
    Connect,
    Quit,
    Shutdown,
    Reboot,
    UpdateSocket(SocketAddr),

    TestPattern(PGenTestPattern),
    MultipleCommandsInfo(Vec<PGenInfoCommand>),
}

#[derive(Debug)]
pub enum PGenCommandResponse {
    NotConnected,
    Busy,
    Ok(bool),
    Errored(String),
    Alive(bool),
    Connect(ConnectState),
    Quit(ConnectState),
    Shutdown(ConnectState),
    Reboot(ConnectState),
    MultipleCommandInfo(Vec<(PGenInfoCommand, String)>),
}

#[derive(Debug)]
pub enum PGenInfoCommand {
    GetResolution,
    GetOutputRange,
    GetColorFormat,
    GetOutputIsSDR,
    GetOutputIsHDR,
    GetOutputIsLLDV,
    GetOutputIsStdDovi,
}

impl PGenInfoCommand {
    pub const fn to_str(&self) -> &str {
        match self {
            Self::GetResolution => "GET_RESOLUTION",
            Self::GetColorFormat => "GET_PGENERATOR_CONF_COLOR_FORMAT",
            Self::GetOutputRange => "GET_OUTPUT_RANGE",
            Self::GetOutputIsSDR => "GET_PGENERATOR_CONF_IS_SDR",
            Self::GetOutputIsHDR => "GET_PGENERATOR_CONF_IS_HDR",
            Self::GetOutputIsLLDV => "GET_PGENERATOR_CONF_IS_LL_DOVI",
            Self::GetOutputIsStdDovi => "GET_PGENERATOR_CONF_IS_STD_DOVI",
        }
    }

    pub fn output_info_commands() -> Vec<Self> {
        vec![
            Self::GetResolution,
            Self::GetColorFormat,
            Self::GetOutputRange,
            Self::GetOutputIsSDR,
            Self::GetOutputIsHDR,
            Self::GetOutputIsLLDV,
            Self::GetOutputIsStdDovi,
        ]
    }

    pub fn split_command_result<'a>(&self, res: &'a str) -> Option<&'a str> {
        let cmd_str = self.to_str();
        res.find(cmd_str).map(|i| {
            // Ignore :
            &res[i + cmd_str.len() + 1..]
        })
    }

    pub fn parse_get_resolution(res: String) -> Option<(u16, u16)> {
        Self::GetResolution
            .split_command_result(&res)
            .and_then(|res| {
                res.split('x')
                    .map(|dim| dim.parse::<u16>())
                    .filter_map(Result::ok)
                    .next_tuple()
            })
    }

    // true = limited, false = full
    pub fn parse_get_output_range(res: String) -> Option<bool> {
        Self::GetOutputRange
            .split_command_result(&res)
            .and_then(|res| res.split_whitespace().last().map(|range| range != "full"))
    }

    pub fn parse_bool_config(&self, res: String) -> bool {
        self.split_command_result(&res)
            .map(|res| res == "1")
            .unwrap_or(false)
    }

    pub fn parse_number_config<T: std::str::FromStr>(&self, res: String) -> Option<T> {
        self.split_command_result(&res)
            .and_then(|res| res.parse::<T>().ok())
    }
}
