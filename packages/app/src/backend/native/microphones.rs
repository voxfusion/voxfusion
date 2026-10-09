//! Which microphone records while a laptop's lid is closed. Closing the lid
//! disconnects the built-in microphone in hardware; macOS still lists it,
//! and keeps it as the default input, but it records silence. A recording
//! that would use it takes another microphone instead.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InputKind {
    /// The laptop's own microphone, which the closed lid disconnects.
    BuiltIn,
    /// A microphone on a cable: USB, Thunderbolt, a display, the headset
    /// jack.
    Wired,
    /// Opening a Bluetooth headset's microphone moves it to its call
    /// profile, which also lowers the quality of what it plays.
    Bluetooth,
    /// Not a microphone of its own: virtual and aggregate devices, and an
    /// iPhone's through Continuity, which opening would connect to.
    Other,
}

/// The UID of the built-in microphone of Macs with a T2 chip or Apple
/// silicon. The headset jack is a device of its own there.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BUILT_IN_MICROPHONE_UID: &str = "BuiltInMicrophoneDevice";

/// The data source of older Macs' built-in input while it records from the
/// internal microphone (`emic` when a headset is plugged in).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const INTERNAL_MICROPHONE_SOURCE: u32 = u32::from_be_bytes(*b"imic");

/// Whether the lid is closed, or `None` on a computer without one.
pub fn lid_state() -> Option<bool> {
    #[cfg(target_os = "macos")]
    return crate::platform::lid::closed();
    #[cfg(not(target_os = "macos"))]
    None
}

pub fn lid_closed() -> bool {
    lid_state() == Some(true)
}

/// Calls `on_change` with whether the lid is now closed each time it opens
/// or closes.
pub fn watch_lid(on_change: fn(bool)) {
    #[cfg(target_os = "macos")]
    crate::platform::lid::watch(on_change);
    #[cfg(not(target_os = "macos"))]
    let _ = on_change;
}

pub fn kind(device: &cpal::Device) -> InputKind {
    #[cfg(target_os = "macos")]
    if let Ok(cpal::DeviceId(_, uid)) = device.id() {
        return super::media::input_connection(&uid)
            .map_or(InputKind::Other, |(transport, source)| {
                classify(&uid, transport, source)
            });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = device;
    InputKind::Other
}

/// The kind of an input device from its UID, its CoreAudio transport type
/// and its data source.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn classify(uid: &str, transport: u32, source: Option<u32>) -> InputKind {
    match &transport.to_be_bytes() {
        b"bltn" if uid == BUILT_IN_MICROPHONE_UID || source == Some(INTERNAL_MICROPHONE_SOURCE) => {
            InputKind::BuiltIn
        }
        // The headset jack and line input.
        b"bltn" => InputKind::Wired,
        b"usb " | b"thun" | b"1394" | b"pci " | b"eavb" | b"hdmi" | b"dprt" => InputKind::Wired,
        b"blue" | b"blea" => InputKind::Bluetooth,
        _ => InputKind::Other,
    }
}

/// The microphone to record from instead of the built-in one: the system
/// default if it is another microphone, else the first wired one, else the
/// first Bluetooth one.
pub fn replacement(host: &cpal::Host) -> Option<(cpal::Device, InputKind)> {
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok());
    let mut devices: Vec<cpal::Device> = host.input_devices().ok()?.collect();
    let kinds: Vec<InputKind> = devices.iter().map(kind).collect();
    let default = default_id.and_then(|default_id| {
        devices
            .iter()
            .position(|device| device.id().ok().as_ref() == Some(&default_id))
    });

    let index = replacement_index(&kinds, default)?;
    Some((devices.swap_remove(index), kinds[index]))
}

/// What [`report`] tells about the microphones.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// `None` on a computer without a lid.
    lid_closed: Option<bool>,
    microphones: Vec<ReportedMicrophone>,
    /// The microphone a recording from the system default would use now.
    records_from: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportedMicrophone {
    name: String,
    kind: InputKind,
    is_default: bool,
}

/// The lid, the microphones and the one a recording would use, for checking
/// a build on a real laptop (see `diagnostics`).
pub fn report() -> Report {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok());
    let name = |device: &cpal::Device| {
        device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_default()
    };

    let microphones = host
        .input_devices()
        .map(|devices| {
            devices
                .map(|device| ReportedMicrophone {
                    name: name(&device),
                    kind: kind(&device),
                    is_default: default_id.is_some() && device.id().ok() == default_id,
                })
                .collect()
        })
        .unwrap_or_default();

    Report {
        lid_closed: lid_state(),
        microphones,
        records_from: super::audio::input_device(&host, None)
            .ok()
            .map(|(device, _)| name(&device)),
    }
}

fn replacement_index(kinds: &[InputKind], default: Option<usize>) -> Option<usize> {
    if let Some(default) = default
        && matches!(
            kinds.get(default),
            Some(InputKind::Wired | InputKind::Bluetooth)
        )
    {
        return Some(default);
    }

    let first = |wanted: InputKind| kinds.iter().position(|kind| *kind == wanted);
    first(InputKind::Wired).or_else(|| first(InputKind::Bluetooth))
}

#[cfg(test)]
mod tests {
    use super::InputKind::*;
    use super::*;

    fn transport(code: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*code)
    }

    #[test]
    fn the_built_in_microphone_is_told_from_the_headset_jack() {
        // Apple silicon and T2: separate devices.
        assert_eq!(
            classify("BuiltInMicrophoneDevice", transport(b"bltn"), None),
            BuiltIn
        );
        assert_eq!(
            classify("BuiltInHeadphoneInputDevice", transport(b"bltn"), None),
            Wired
        );
        // Older Macs: one device whose source follows the jack.
        let older = "AppleHDAEngineInput:1B,0,1,0:1";
        assert_eq!(
            classify(older, transport(b"bltn"), Some(transport(b"imic"))),
            BuiltIn
        );
        assert_eq!(
            classify(older, transport(b"bltn"), Some(transport(b"emic"))),
            Wired
        );
    }

    #[test]
    fn devices_are_sorted_by_how_they_connect() {
        assert_eq!(classify("usb-mic", transport(b"usb "), None), Wired);
        assert_eq!(classify("display", transport(b"thun"), None), Wired);
        assert_eq!(classify("airpods", transport(b"blue"), None), Bluetooth);
        assert_eq!(classify("buds", transport(b"blea"), None), Bluetooth);
        assert_eq!(
            classify("BlackHole2ch_UID", transport(b"virt"), None),
            Other
        );
        assert_eq!(classify("aggregate", transport(b"grup"), None), Other);
        assert_eq!(classify("iphone", transport(b"ccwl"), None), Other);
        assert_eq!(classify("unknown", 0, None), Other);
    }

    #[test]
    fn the_system_default_is_kept_when_it_is_another_microphone() {
        assert_eq!(
            replacement_index(&[BuiltIn, Wired, Bluetooth], Some(2)),
            Some(2)
        );
    }

    #[test]
    fn a_wired_microphone_comes_before_bluetooth() {
        assert_eq!(
            replacement_index(&[Bluetooth, BuiltIn, Other, Wired], Some(1)),
            Some(3)
        );
        assert_eq!(replacement_index(&[Bluetooth, BuiltIn], None), Some(0));
    }

    #[test]
    fn virtual_devices_never_replace_the_built_in_microphone() {
        assert_eq!(replacement_index(&[BuiltIn, Other], Some(1)), None);
        assert_eq!(replacement_index(&[BuiltIn], Some(0)), None);
        assert_eq!(replacement_index(&[], None), None);
    }
}
