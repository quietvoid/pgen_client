use std::{io::BufReader, net::TcpStream};

use yaserde::YaDeserialize;

use crate::pgen::{
    client::PGenTestPattern, controller::PGenControllerHandle, pattern_config::PGenPatternConfig,
};

use super::GeneratorClientHandle;

pub async fn resolve_connect_and_set_tcp_stream(generator_client: GeneratorClientHandle) -> bool {
    let addr = "127.0.0.1:20002";
    match TcpStream::connect(addr) {
        Ok(stream) => {
            {
                let mut client = generator_client.write().await;
                client.stream.replace(BufReader::new(stream));
            }

            log::trace!("Started Resolve TCP connection");
            true
        }
        Err(e) => {
            log::error!("{e:?}");
            false
        }
    }
}

pub fn handle_resolve_pattern_message(controller: PGenControllerHandle, msg: &str) {
    let pattern = yaserde::de::from_str::<ResolvePattern>(msg);

    match pattern {
        Ok(pattern) => {
            if let Ok(mut controller) = controller.write() {
                let config = pattern.to_pgen(controller.state.pattern_config);
                let pgen_pattern =
                    PGenTestPattern::from_config(controller.get_color_format(), &config);
                controller.send_pattern(pgen_pattern);
            }
        }
        Err(e) => log::error!("{e}"),
    }
}

#[derive(Debug, Default, YaDeserialize)]
#[yaserde(root = "calibration")]
pub struct ResolvePattern {
    color: ResolveColor,
    background: ResolveColor,
    #[allow(dead_code)]
    geometry: ResolveGeometry,
}

#[derive(Debug, Default, YaDeserialize)]
struct ResolveColor {
    #[yaserde(attribute)]
    red: u16,
    #[yaserde(attribute)]
    green: u16,
    #[yaserde(attribute)]
    blue: u16,
    #[yaserde(attribute)]
    bits: u8,
}

#[derive(Debug, Default, YaDeserialize)]
#[allow(dead_code)]
struct ResolveGeometry {
    #[yaserde(attribute)]
    x: f32,
    #[yaserde(attribute)]
    y: f32,
    #[yaserde(attribute, rename = "cx")]
    w: f32,
    #[yaserde(attribute, rename = "cy")]
    h: f32,
}

impl ResolvePattern {
    pub fn to_pgen(&self, base: PGenPatternConfig) -> PGenPatternConfig {
        PGenPatternConfig {
            bit_depth: self.color.bits,
            patch_colour: self.color.to_array(),
            background_colour: self.background.to_array(),
            ..base
        }
    }
}

impl ResolveColor {
    pub fn to_array(&self) -> [u16; 3] {
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
