use std::net::SocketAddr;

use strum::{AsRefStr, Display};

use super::{
    client::{ConnectState, PGenTestPattern},
    controller::DisplayMode,
    BitDepth, ColorFormat, Colorimetry, DoviMapMode, DynamicRange, HdrEotf, Primaries, QuantRange,
};

#[derive(Debug, Clone)]
pub enum PGenCommand {
    IsAlive,
    Connect,
    Quit,
    Shutdown,
    Reboot,
    UpdateSocket(SocketAddr),
    RestartSoftware,
    TestPattern(PGenTestPattern),
    MultipleGetConfCommands(&'static [PGenGetConfCommand]),
    MultipleSetConfCommands(Vec<PGenSetConfCommand>),
}

#[derive(Debug, Clone)]
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
    MultipleGetConfRes(Vec<(PGenGetConfCommand, String)>),
    // true if the config was properly set
    MultipleSetConfRes(Vec<(PGenSetConfCommand, bool)>),
}

#[derive(Display, AsRefStr, Debug, Copy, Clone)]
pub enum PGenGetConfCommand {
    #[strum(to_string = "GET_PGENERATOR_VERSION")]
    GetPGeneratorVersion,
    #[strum(to_string = "GET_PGENERATOR_IS_EXECUTED")]
    GetPGeneratorPid,
    #[strum(to_string = "GET_MODE")]
    GetCurrentMode,
    #[strum(to_string = "GET_MODES_AVAILABLE")]
    GetModesAvailable,

    #[strum(to_string = "GET_PGENERATOR_CONF_COLOR_FORMAT")]
    GetColorFormat,
    #[strum(to_string = "GET_PGENERATOR_CONF_MAX_BPC")]
    GetBitDepth,
    #[strum(to_string = "GET_PGENERATOR_CONF_RGB_QUANT_RANGE")]
    GetQuantRange,
    #[strum(to_string = "GET_PGENERATOR_CONF_COLORIMETRY")]
    GetColorimetry,
    #[strum(to_string = "GET_PGENERATOR_CONF_IS_SDR")]
    GetOutputIsSDR,
    #[strum(to_string = "GET_PGENERATOR_CONF_IS_HDR")]
    GetOutputIsHDR,
    #[strum(to_string = "GET_PGENERATOR_CONF_IS_LL_DOVI")]
    GetOutputIsLLDV,
    #[strum(to_string = "GET_PGENERATOR_CONF_IS_STD_DOVI")]
    GetOutputIsStdDovi,
    #[strum(to_string = "GET_PGENERATOR_CONF_DV_MAP_MODE")]
    GetDoviMapMode,

    // HDR metadata infoframe
    #[strum(to_string = "GET_PGENERATOR_CONF_EOTF")]
    GetHdrEotf,
    #[strum(to_string = "GET_PGENERATOR_CONF_PRIMARIES")]
    GetHdrPrimaries,
    #[strum(to_string = "GET_PGENERATOR_CONF_MAX_LUMA")]
    GetHdrMaxMdl,
    #[strum(to_string = "GET_PGENERATOR_CONF_MIN_LUMA")]
    GetHdrMinMdl,
    #[strum(to_string = "GET_PGENERATOR_CONF_MAX_CLL")]
    GetHdrMaxCLL,
    #[strum(to_string = "GET_PGENERATOR_CONF_MAX_FALL")]
    GetHdrMaxFALL,

    // Always returns "RGB Full", not used
    #[strum(to_string = "GET_OUTPUT_RANGE")]
    GetOutputRange,
}

#[derive(Display, AsRefStr, Debug, Copy, Clone)]
pub enum PGenSetConfCommand {
    #[strum(to_string = "SET_MODE")]
    SetDisplayMode(DisplayMode),
    #[strum(to_string = "SET_PGENERATOR_CONF_COLOR_FORMAT")]
    SetColorFormat(ColorFormat),
    #[strum(to_string = "SET_PGENERATOR_CONF_MAX_BPC")]
    SetBitDepth(BitDepth),
    #[strum(to_string = "SET_PGENERATOR_CONF_RGB_QUANT_RANGE")]
    SetQuantRange(QuantRange),
    #[strum(to_string = "SET_PGENERATOR_CONF_COLORIMETRY")]
    SetColorimetry(Colorimetry),

    #[strum(to_string = "SET_PGENERATOR_CONF_IS_SDR")]
    SetOutputIsSDR(bool),
    #[strum(to_string = "SET_PGENERATOR_CONF_IS_HDR")]
    SetOutputIsHDR(bool),
    #[strum(to_string = "SET_PGENERATOR_CONF_IS_LL_DOVI")]
    SetOutputIsLLDV(bool),
    #[strum(to_string = "SET_PGENERATOR_CONF_IS_STD_DOVI")]
    SetOutputIsStdDovi(bool),
    #[strum(to_string = "SET_PGENERATOR_CONF_DV_STATUS")]
    SetDoviStatus(bool),
    #[strum(to_string = "SET_PGENERATOR_CONF_DV_INTERFACE")]
    SetDoviInterface(bool),
    #[strum(to_string = "SET_PGENERATOR_CONF_DV_MAP_MODE")]
    SetDoviMapMode(DoviMapMode),

    // HDR metadata infoframe
    #[strum(to_string = "SET_PGENERATOR_CONF_EOTF")]
    SetHdrEotf(HdrEotf),
    #[strum(to_string = "SET_PGENERATOR_CONF_PRIMARIES")]
    SetHdrPrimaries(Primaries),
    #[strum(to_string = "SET_PGENERATOR_CONF_MAX_LUMA")]
    SetHdrMaxMdl(u16),
    #[strum(to_string = "SET_PGENERATOR_CONF_MIN_LUMA")]
    SetHdrMinMdl(u16),
    #[strum(to_string = "SET_PGENERATOR_CONF_MAX_CLL")]
    SetHdrMaxCLL(u16),
    #[strum(to_string = "SET_PGENERATOR_CONF_MAX_FALL")]
    SetHdrMaxFALL(u16),
}

impl PGenGetConfCommand {
    pub const fn base_info_commands() -> &'static [Self] {
        &[
            Self::GetPGeneratorVersion,
            Self::GetPGeneratorPid,
            Self::GetModesAvailable,
            Self::GetCurrentMode,
            Self::GetColorFormat,
            Self::GetBitDepth,
            Self::GetQuantRange,
            Self::GetColorimetry,
            Self::GetOutputIsSDR,
            Self::GetOutputIsHDR,
            Self::GetOutputIsLLDV,
            Self::GetOutputIsStdDovi,
            Self::GetDoviMapMode,
            Self::GetHdrEotf,
            Self::GetHdrPrimaries,
            Self::GetHdrMaxMdl,
            Self::GetHdrMinMdl,
            Self::GetHdrMaxCLL,
            Self::GetHdrMaxFALL,
        ]
    }

    pub fn split_command_result<'a>(&self, res: &'a str) -> Option<&'a str> {
        let cmd_str = self.as_ref();
        res.find(cmd_str).map(|i| {
            // Ignore :
            &res[i + cmd_str.len() + 1..]
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

    pub fn parse_string_config<'a>(&self, res: &'a str) -> &'a str {
        self.split_command_result(res).unwrap_or("Unknown")
    }
}

impl PGenSetConfCommand {
    pub const fn value(self) -> usize {
        match self {
            Self::SetDisplayMode(mode) => mode.id,
            Self::SetColorFormat(format) => format as usize,
            Self::SetBitDepth(bit_depth) => bit_depth as usize,
            Self::SetQuantRange(quant_range) => quant_range as usize,
            Self::SetColorimetry(colorimetry) => colorimetry as usize,
            Self::SetOutputIsSDR(is_sdr) => is_sdr as usize,
            Self::SetOutputIsHDR(is_hdr) => is_hdr as usize,
            Self::SetOutputIsLLDV(is_lldv) => is_lldv as usize,
            Self::SetOutputIsStdDovi(is_std_dovi) => is_std_dovi as usize,
            Self::SetDoviStatus(dovi_status) => dovi_status as usize,
            Self::SetDoviInterface(dovi_interface) => dovi_interface as usize,
            Self::SetDoviMapMode(dovi_map_mode) => dovi_map_mode as usize,
            Self::SetHdrEotf(eotf) => eotf as usize,
            Self::SetHdrPrimaries(primaries) => primaries as usize,
            Self::SetHdrMaxMdl(max_mdl) => max_mdl as usize,
            Self::SetHdrMinMdl(min_mdl) => min_mdl as usize,
            Self::SetHdrMaxCLL(maxcll) => maxcll as usize,
            Self::SetHdrMaxFALL(maxfall) => maxfall as usize,
        }
    }

    fn base_config_for_dynamic_range(dynamic_range: DynamicRange) -> (bool, bool, bool, Vec<Self>) {
        let is_sdr = dynamic_range == DynamicRange::Sdr;
        let is_hdr = dynamic_range == DynamicRange::Hdr;
        let is_dovi = dynamic_range == DynamicRange::Dovi;

        let commands = vec![
            Self::SetOutputIsSDR(is_sdr),
            Self::SetOutputIsHDR(is_hdr || is_dovi),
            Self::SetOutputIsLLDV(is_dovi),
            Self::SetOutputIsStdDovi(is_dovi),
            Self::SetDoviStatus(is_dovi),
            Self::SetDoviInterface(is_dovi),
        ];

        (is_sdr, is_hdr, is_dovi, commands)
    }

    const fn default_sdr_config() -> &'static [Self] {
        &[PGenSetConfCommand::SetColorimetry(Colorimetry::Bt709Ycc)]
    }

    const fn default_hdr_config() -> &'static [Self] {
        &[
            Self::SetColorimetry(Colorimetry::Bt2020Rgb),
            Self::SetBitDepth(BitDepth::Ten),
            Self::SetHdrPrimaries(Primaries::DisplayP3),
        ]
    }

    const fn dovi_config() -> &'static [Self] {
        &[
            Self::SetColorFormat(ColorFormat::Rgb),
            Self::SetQuantRange(QuantRange::Full),
            Self::SetBitDepth(BitDepth::Eight),
            Self::SetColorimetry(Colorimetry::Bt709Ycc),
            Self::SetDoviMapMode(DoviMapMode::Absolute),
            Self::SetHdrPrimaries(Primaries::Rec2020),
        ]
    }

    pub fn commands_for_dynamic_range(dynamic_range: DynamicRange) -> Vec<Self> {
        let (is_sdr, is_hdr, is_dovi, mut commands) =
            Self::base_config_for_dynamic_range(dynamic_range);

        // Set default configs
        if is_sdr {
            commands.extend(Self::default_sdr_config());
        } else if is_hdr {
            commands.extend(Self::default_hdr_config());
        } else if is_dovi {
            // Required params for DoVi
            commands.extend(Self::dovi_config());
        }

        commands
    }
}
