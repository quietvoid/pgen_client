use std::io;
use std::time::Duration;

use tokio::sync::mpsc::Sender;
use tokio::{net::TcpStream, time::timeout};
use yaserde::YaDeserialize;

use crate::pgen::BitDepth;
use crate::pgen::{controller::PGenControllerCmd, pattern_config::PGenPatternConfig};
use crate::utils::Rgb;

const RESOLVE_INTERFACE_ADDRESS: &str = "127.0.0.1:20002";

pub async fn resolve_connect_tcp_stream() -> io::Result<TcpStream> {
    timeout(Duration::from_secs(5), async {
        TcpStream::connect(RESOLVE_INTERFACE_ADDRESS).await
    })
    .await?
}

pub async fn handle_resolve_tcp_stream_message(tcp_stream: &mut TcpStream) -> io::Result<String> {
    let mut header = [0; 4];
    let n = tcp_stream.try_read(&mut header)?;
    if n != 4 {
        return Err(io::Error::other("Resolve: invalid header"));
    }

    let msg_len = u32::from_be_bytes(header) as usize;
    let mut msg = vec![0_u8; msg_len];
    let n = tcp_stream.try_read(msg.as_mut())?;
    if n != msg_len {
        return Err(io::Error::other("Resolve: Failed reading packet"));
    }

    // Shouldn't fail
    let msg = String::from_utf8(msg).unwrap();

    Ok(msg)
}

pub async fn handle_resolve_pattern_message(controller_tx: &Sender<PGenControllerCmd>, msg: &str) {
    let pattern = yaserde::de::from_str::<ResolvePattern>(msg);

    match pattern {
        Ok(pattern) => {
            log::debug!("Resolve pattern received: {pattern:?}");
            let config = pattern.to_pgen();
            let cmd = PGenControllerCmd::TestPattern(config);
            controller_tx.try_send(cmd).ok();
        }
        Err(e) => log::error!("{e}"),
    }
}

#[derive(Debug, Default, YaDeserialize)]
pub struct ResolvePattern {
    color: ResolveColor,
    background: ResolveColor,
    #[allow(dead_code)]
    geometry: ResolveGeometry,
}

#[derive(Debug, Default, YaDeserialize)]
struct ResolveColor {
    #[yaserde(attribute = true)]
    red: u16,
    #[yaserde(attribute = true)]
    green: u16,
    #[yaserde(attribute = true)]
    blue: u16,
    #[yaserde(attribute = true)]
    bits: u8,
}

#[derive(Debug, Default, YaDeserialize)]
#[allow(dead_code)]
struct ResolveGeometry {
    #[yaserde(attribute = true)]
    x: f32,
    #[yaserde(attribute = true)]
    y: f32,
    #[yaserde(attribute = true, rename = "cx")]
    w: f32,
    #[yaserde(attribute = true, rename = "cy")]
    h: f32,
}

impl ResolvePattern {
    pub fn to_pgen(&self) -> PGenPatternConfig {
        PGenPatternConfig {
            bit_depth: BitDepth::from_repr(self.color.bits as usize).unwrap(),
            patch_colour: self.color.to_array(),
            background_colour: self.background.to_array(),
            ..Default::default()
        }
    }
}

impl ResolveColor {
    pub fn to_array(&self) -> Rgb {
        [self.red, self.green, self.blue]
    }
}

impl ResolveGeometry {
    #[allow(dead_code)]
    pub fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }
}

#[cfg(test)]
mod tests {
    use super::ResolvePattern;

    #[test]
    fn parse_xml_string() {
        let msg = r#"<?xml version="1.0" encoding="utf-8"?><calibration>
        <color red="235" green="235" blue="235" bits="10"/>
        <background red="16" green="16" blue="16" bits="10"/>
        <geometry x="0.0000" y="0.0000" cx="1920.0000" cy="1080.0000"/>
        </calibration>"#;

        let pattern: ResolvePattern = yaserde::de::from_str(msg).unwrap();
        assert_eq!(pattern.color.to_array(), [235, 235, 235]);
        assert_eq!(pattern.color.bits, 10);
        assert_eq!(pattern.background.to_array(), [16, 16, 16]);
        assert_eq!(pattern.background.bits, 10);
        assert_eq!(pattern.geometry.to_array(), [0.0, 0.0, 1920.0, 1080.0]);
    }
}
