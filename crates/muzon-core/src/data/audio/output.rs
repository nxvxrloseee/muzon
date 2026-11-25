//! Direct ALSA routing and observational status. Caps never prove bit identity.
use crate::domain::playback_settings::{validate_output_device, OutputMode};
use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct SinkFormat {
    pub format: String,
    pub rate: i32,
    pub channels: i32,
}

#[derive(Debug, Clone, Default, Serialize, Type)]
pub struct OutputStatus {
    /// Applied pipeline configuration (playback can be stopped/blocked).
    pub mode: OutputMode,
    /// Most recent routing request, including a failed attempt.
    pub requested_mode: OutputMode,
    pub requested_device: Option<String>,
    pub blocked: bool,
    pub device: Option<String>,
    pub negotiated_format: Option<SinkFormat>,
    /// Always false: this engine has no hardware sample-comparison evidence.
    pub bit_perfect_verified: bool,
    pub error: Option<String>,
}

pub(crate) fn sink_format(sink: &gst::Element) -> Result<Option<SinkFormat>, String> {
    let Some(caps) = sink.static_pad("sink").and_then(|pad| pad.current_caps()) else {
        return Ok(None);
    };
    format_from_caps(&caps).map(Some)
}

fn format_from_caps(caps: &gst::CapsRef) -> Result<SinkFormat, String> {
    let unexpected = || format!("Unexpected negotiated sink caps (PCM required): {caps}");
    let s = caps.structure(0).ok_or_else(unexpected)?;
    if !caps.is_fixed() || s.name() != "audio/x-raw" {
        return Err(unexpected());
    }
    let format = SinkFormat {
        format: s.get::<String>("format").map_err(|_| unexpected())?,
        rate: s.get("rate").map_err(|_| unexpected())?,
        channels: s.get("channels").map_err(|_| unexpected())?,
    };
    if format.rate <= 0 || format.channels <= 0 {
        return Err(unexpected());
    }
    Ok(format)
}

/// Enumerates only ALSA hardware sinks. No default, plughw, PulseAudio or
/// PipeWire aliases are ever upgraded into a supposedly direct device.
pub fn list_output_devices() -> anyhow::Result<Vec<OutputDevice>> {
    gst::init()?;
    let fallback = proc_output_devices(std::path::Path::new("/proc/asound"));
    let monitor = gst::DeviceMonitor::new();
    if monitor.add_filter(Some("Audio/Sink"), None).is_none() {
        anyhow::ensure!(
            !fallback.is_empty(),
            "No Audio/Sink device provider or ALSA playback endpoints"
        );
        return Ok(fallback);
    }
    if let Err(error) = monitor.start() {
        monitor.stop();
        if !fallback.is_empty() {
            return Ok(fallback);
        }
        return Err(error.into());
    }
    let result = {
        let mut result = Vec::new();
        for device in monitor.devices() {
            let Some(props) = device.properties() else {
                continue;
            };
            if props.get::<String>("device.api").ok().as_deref() != Some("alsa") {
                continue;
            }
            // create_element constructs a sink but never changes its state or
            // opens the PCM. It is the authoritative device string source.
            let Ok(element) = device.create_element(None) else {
                continue;
            };
            if element.factory().is_none_or(|f| f.name() != "alsasink") {
                continue;
            }
            let raw = element.property::<String>("device");
            let Some(id) = stable_device_id(&raw, props.get::<String>("alsa.id").ok().as_deref())
            else {
                continue;
            };
            result.push(OutputDevice {
                id,
                name: device.display_name().to_string(),
            });
        }
        result.extend(fallback);
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result.dedup_by(|a, b| a.id == b.id);
        result
    };
    monitor.stop();
    Ok(result)
}

/// Linux read-only supplement when the registry only exposes system-server
/// sinks. Read procfs, never open /dev/snd or query a PCM with ALSA.
fn proc_output_devices(root: &std::path::Path) -> Vec<OutputDevice> {
    let Ok(pcm) = std::fs::read_to_string(root.join("pcm")) else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for (card, dev, name) in parse_playback_endpoints(&pcm) {
        let Ok(id) = std::fs::read_to_string(root.join(format!("card{card}/id"))) else {
            continue;
        };
        let id = format!("hw:CARD={},DEV={dev}", id.trim());
        if validate_output_device(&id).is_err() {
            continue;
        }
        let info = std::fs::read_to_string(root.join(format!("card{card}/pcm{dev}p/info")))
            .unwrap_or_default();
        let name = info
            .lines()
            .find_map(|line| line.strip_prefix("name:").map(str::trim))
            .filter(|s| !s.is_empty())
            .unwrap_or(&name);
        devices.push(OutputDevice {
            name: format!("{name} ({id})"),
            id,
        });
    }
    devices
}

fn parse_playback_endpoints(pcm: &str) -> Vec<(u32, u32, String)> {
    pcm.lines()
        .filter_map(|line| {
            let mut fields = line.split(':');
            let address = fields.next()?.trim();
            let name = fields.next()?.trim().to_string();
            let playback = fields.any(|field| {
                let mut words = field.split_whitespace();
                words.next() == Some("playback")
                    && words
                        .next()
                        .and_then(|v| v.parse::<u32>().ok())
                        .is_some_and(|n| n > 0)
            });
            if !playback {
                return None;
            }
            let (card, dev) = address.split_once('-')?;
            Some((card.parse().ok()?, dev.parse().ok()?, name))
        })
        .collect()
}

fn stable_device_id(raw: &str, card_id: Option<&str>) -> Option<String> {
    validate_output_device(raw).ok()?;
    if raw.starts_with("hw:CARD=") {
        return Some(raw.into());
    }
    let (_, dev) = raw.strip_prefix("hw:")?.split_once(',')?;
    // Never persist an enumerated card index: it changes with hotplug order.
    let id = format!("hw:CARD={},DEV={dev}", card_id?);
    validate_output_device(&id).ok()?;
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proc_parser_lists_only_playback_endpoints() {
        let pcm = "00-03: HDMI 0 : HDMI 0 : playback 1\n01-00: USB Audio : USB Audio : playback 1 : capture 1\n02-02: Mic : Mic : capture 1\n03-00: Disabled : Disabled : playback 0\nbroken: ignored : playback nope\n";
        assert_eq!(
            parse_playback_endpoints(pcm),
            vec![(0, 3, "HDMI 0".into()), (1, 0, "USB Audio".into())]
        );
    }
    #[test]
    fn proc_fallback_builds_named_ids_and_tolerates_disappearing_cards() {
        let dir = crate::testing::scratch_dir("proc-audio-devices");
        std::fs::create_dir_all(dir.join("card1/pcm0p")).unwrap();
        std::fs::write(
            dir.join("pcm"),
            "01-00: USB Audio : USB Audio : playback 1\n02-00: Gone : Gone : playback 1\n",
        )
        .unwrap();
        std::fs::write(dir.join("card1/id"), "DSP\n").unwrap();
        std::fs::write(
            dir.join("card1/pcm0p/info"),
            "id: USB Audio\nname: Chu2 DSP\n",
        )
        .unwrap();
        assert_eq!(
            proc_output_devices(&dir),
            vec![OutputDevice {
                id: "hw:CARD=DSP,DEV=0".into(),
                name: "Chu2 DSP (hw:CARD=DSP,DEV=0)".into()
            }]
        );
    }
    #[test]
    fn enumeration_requires_stable_direct_hardware_identity() {
        assert_eq!(
            stable_device_id("hw:2,0", Some("USB")),
            Some("hw:CARD=USB,DEV=0".into())
        );
        for raw in ["default", "plughw:2,0", "pulse", "hw:2", "hw:2,0,1"] {
            assert_eq!(stable_device_id(raw, Some("USB")), None);
        }
        assert_eq!(stable_device_id("hw:2,0", None), None);
    }
}
