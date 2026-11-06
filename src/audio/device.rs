use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelPair {
    pub label: String,
    pub left_channel: usize,
    pub right_channel: usize,
}

pub fn list_input_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(name) = device.name() {
                let (channels, sample_rate) = match device.default_input_config() {
                    Ok(cfg) => (cfg.channels(), cfg.sample_rate().0),
                    Err(_) => (2, 44100),
                };
                devices.push(AudioDeviceInfo {
                    name,
                    channels,
                    sample_rate,
                });
            }
        }
    }
    devices
}

pub fn get_channel_pairs_for_device(dev: &AudioDeviceInfo) -> Vec<ChannelPair> {
    let mut pairs = Vec::new();
    let is_zoom_l12 = dev.name.to_lowercase().contains("l-12") || dev.name.to_lowercase().contains("zoom");

    if is_zoom_l12 && dev.channels >= 14 {
        // Zoom LiveTrak L-12 explicit layout
        pairs.push(ChannelPair {
            label: "Kanäle 11/12 (Stereo Line - Standard für Tape)".into(),
            left_channel: 10,
            right_channel: 11,
        });
        pairs.push(ChannelPair {
            label: "Kanäle 9/10 (Stereo Line)".into(),
            left_channel: 8,
            right_channel: 9,
        });
        pairs.push(ChannelPair {
            label: "Kanäle 13/14 (Master Mix L/R)".into(),
            left_channel: 12,
            right_channel: 13,
        });
        pairs.push(ChannelPair {
            label: "Kanäle 1/2 (Mono Eingänge 1 & 2)".into(),
            left_channel: 0,
            right_channel: 1,
        });
        pairs.push(ChannelPair {
            label: "Kanäle 3/4 (Mono Eingänge 3 & 4)".into(),
            left_channel: 2,
            right_channel: 3,
        });
        pairs.push(ChannelPair {
            label: "Kanäle 5/6 (Mono Eingänge 5 & 6)".into(),
            left_channel: 4,
            right_channel: 5,
        });
        pairs.push(ChannelPair {
            label: "Kanäle 7/8 (Mono Eingänge 7 & 8)".into(),
            left_channel: 6,
            right_channel: 7,
        });
    } else if dev.channels == 1 {
        pairs.push(ChannelPair {
            label: "Kanal 1 (Mono auf Stereo L/R)".into(),
            left_channel: 0,
            right_channel: 0,
        });
    } else {
        // Generic multi-channel or stereo
        for i in (0..dev.channels).step_by(2) {
            let left = i as usize;
            let right = ((i + 1).min(dev.channels - 1)) as usize;
            pairs.push(ChannelPair {
                label: format!("Kanäle {} / {}", left + 1, right + 1),
                left_channel: left,
                right_channel: right,
            });
        }
    }

    if pairs.is_empty() {
        pairs.push(ChannelPair {
            label: "Kanal 1 / 2 (Stereo)".into(),
            left_channel: 0,
            right_channel: 1,
        });
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        let devices = list_input_devices();
        println!("Gefundene Audio-Eingabegeräte ({}):", devices.len());
        for d in &devices {
            println!(" - {} (Kanäle: {}, Rate: {} Hz)", d.name, d.channels, d.sample_rate);
            let pairs = get_channel_pairs_for_device(d);
            for p in pairs {
                println!("    * {}", p.label);
            }
        }
        let host = cpal::default_host();
        for dev in host.input_devices().unwrap() {
            if let Ok(cfg) = dev.default_input_config() {
                println!("DEV '{}' format: {:?}", dev.name().unwrap_or_default(), cfg.sample_format());
            }
        }
    }
}
