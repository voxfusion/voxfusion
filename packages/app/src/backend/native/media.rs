use std::sync::{LazyLock, Mutex};

#[cfg(target_os = "macos")]
mod filter;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod muffle;
#[cfg(target_os = "macos")]
mod tap;

#[cfg(target_os = "linux")]
use super::pulse;

#[derive(Default)]
struct MediaMuteState {
    active: bool,
    /// Output device muted for the current recording. `None` when it was
    /// already muted, so restoring leaves it alone, or when it has no mute
    /// control and the muffle worker silenced it instead.
    #[cfg(target_os = "macos")]
    muted_device: Option<MutedDevice>,
    /// UIDs of devices that were unplugged while muted. macOS can restore a
    /// device's saved mute state when it reconnects, so they are unmuted once
    /// they are back.
    #[cfg(target_os = "macos")]
    pending_unmute_uids: Vec<String>,
    /// Name of the sink muted for the current recording, `None` when it was
    /// already muted.
    #[cfg(target_os = "linux")]
    muted_sink: Option<String>,
    /// Names of sinks that were unplugged while muted. The sound server
    /// remembers a device's mute, so they are unmuted once they are back.
    #[cfg(target_os = "linux")]
    pending_unmute_sinks: Vec<String>,
}

#[cfg(target_os = "macos")]
struct MutedDevice {
    id: AudioObjectId,
    uid: Option<String>,
}

static MEDIA_MUTE_STATE: LazyLock<Mutex<MediaMuteState>> =
    LazyLock::new(|| Mutex::new(MediaMuteState::default()));

#[cfg(target_os = "macos")]
static EVENTS: std::sync::OnceLock<crate::backend::EventSender> = std::sync::OnceLock::new();

/// The output the muffle worker changes.
#[cfg(target_os = "macos")]
type SystemOutput = CoreAudioOutput;
#[cfg(target_os = "linux")]
type SystemOutput = PulseOutput;

#[cfg(target_os = "macos")]
type AudioObjectId = u32;

#[cfg(target_os = "macos")]
type AudioObjectPropertySelector = u32;

#[cfg(target_os = "macos")]
type AudioObjectPropertyScope = u32;

#[cfg(target_os = "macos")]
type AudioObjectPropertyElement = u32;

#[cfg(target_os = "macos")]
type OsStatus = i32;

#[cfg(target_os = "macos")]
type AudioObjectPropertyListenerProc = unsafe extern "C" fn(
    in_object_id: AudioObjectId,
    in_number_addresses: u32,
    in_addresses: *const AudioObjectPropertyAddress,
    in_client_data: *mut std::ffi::c_void,
) -> OsStatus;

#[cfg(target_os = "macos")]
#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    element: AudioObjectPropertyElement,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        in_object_id: AudioObjectId,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const std::ffi::c_void,
        io_data_size: *mut u32,
        out_data: *mut std::ffi::c_void,
    ) -> OsStatus;

    fn AudioObjectGetPropertyDataSize(
        in_object_id: AudioObjectId,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const std::ffi::c_void,
        out_data_size: *mut u32,
    ) -> OsStatus;

    fn AudioObjectIsPropertySettable(
        in_object_id: AudioObjectId,
        in_address: *const AudioObjectPropertyAddress,
        out_is_settable: *mut u8,
    ) -> OsStatus;

    fn AudioObjectSetPropertyData(
        in_object_id: AudioObjectId,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const std::ffi::c_void,
        in_data_size: u32,
        in_data: *const std::ffi::c_void,
    ) -> OsStatus;

    fn AudioObjectAddPropertyListener(
        in_object_id: AudioObjectId,
        in_address: *const AudioObjectPropertyAddress,
        in_listener: AudioObjectPropertyListenerProc,
        in_client_data: *mut std::ffi::c_void,
    ) -> OsStatus;
}

#[cfg(target_os = "macos")]
const AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectId = 1;

#[cfg(target_os = "macos")]
const AUDIO_OBJECT_UNKNOWN: AudioObjectId = 0;

#[cfg(target_os = "macos")]
const AUDIO_HARDWARE_PROPERTY_DEVICES: AudioObjectPropertySelector = u32::from_be_bytes(*b"dev#");

#[cfg(target_os = "macos")]
const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"dOut");

#[cfg(target_os = "macos")]
const AUDIO_HARDWARE_PROPERTY_TRANSLATE_UID_TO_DEVICE: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"uidd");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_DEVICE_UID: AudioObjectPropertySelector = u32::from_be_bytes(*b"uid ");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_MUTE: AudioObjectPropertySelector = u32::from_be_bytes(*b"mute");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_STREAMS: AudioObjectPropertySelector = u32::from_be_bytes(*b"stm#");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_TRANSPORT_TYPE: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"tran");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"gone");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_TRANSPORT_TYPE_BLUETOOTH: u32 = u32::from_be_bytes(*b"blue");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_TRANSPORT_TYPE_BLUETOOTH_LE: u32 = u32::from_be_bytes(*b"blea");

/// The volume behind the system volume slider, from 0 to 1. Unlike a device's
/// per-channel volumes it exists on any output with adjustable volume, but not
/// on outputs whose volume is fixed, such as many USB audio interfaces.
#[cfg(target_os = "macos")]
const AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"vmvc");

#[cfg(target_os = "macos")]
const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope = u32::from_be_bytes(*b"glob");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: AudioObjectPropertyScope = u32::from_be_bytes(*b"outp");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_SCOPE_INPUT: AudioObjectPropertyScope = u32::from_be_bytes(*b"inpt");

#[cfg(target_os = "macos")]
const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;

#[cfg(target_os = "macos")]
fn core_audio_error(context: &str, status: OsStatus) -> String {
    format!("{} failed with CoreAudio status {}", context, status)
}

#[cfg(target_os = "macos")]
fn global_address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector,
        scope: AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

#[cfg(target_os = "macos")]
fn get_default_output_device() -> Result<AudioObjectId, String> {
    let address = global_address(AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE);
    let mut device_id: AudioObjectId = 0;
    let mut data_size = size_of_val_u32(&device_id)?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            (&mut device_id as *mut AudioObjectId).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Getting default output device", status));
    }

    Ok(device_id)
}

#[cfg(target_os = "macos")]
fn get_device_uid(device_id: AudioObjectId) -> Result<String, String> {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};

    let address = global_address(AUDIO_DEVICE_PROPERTY_DEVICE_UID);
    let mut uid: CFStringRef = std::ptr::null();
    let mut data_size = size_of_val_u32(&uid)?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            (&mut uid as *mut CFStringRef).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Getting device UID", status));
    }
    if uid.is_null() {
        return Err("Device UID is null".to_string());
    }

    // The HAL returns a retained copy of the UID string.
    Ok(unsafe { CFString::wrap_under_create_rule(uid) }.to_string())
}

#[cfg(target_os = "macos")]
fn find_device_by_uid(uid: &str) -> Option<AudioObjectId> {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};

    let address = global_address(AUDIO_HARDWARE_PROPERTY_TRANSLATE_UID_TO_DEVICE);
    let uid = CFString::new(uid);
    let uid_ref: CFStringRef = uid.as_concrete_TypeRef();
    let mut device_id: AudioObjectId = AUDIO_OBJECT_UNKNOWN;
    let mut data_size = size_of_val_u32(&device_id).ok()?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            size_of_val_u32(&uid_ref).ok()?,
            (&uid_ref as *const CFStringRef).cast(),
            &mut data_size,
            (&mut device_id as *mut AudioObjectId).cast(),
        )
    };

    (status == 0 && device_id != AUDIO_OBJECT_UNKNOWN).then_some(device_id)
}

#[cfg(target_os = "macos")]
fn size_of_val_u32<T>(value: &T) -> Result<u32, String> {
    u32::try_from(std::mem::size_of_val(value))
        .map_err(|_| "CoreAudio data size overflow".to_string())
}

#[cfg(target_os = "macos")]
fn output_mute_address() -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector: AUDIO_DEVICE_PROPERTY_MUTE,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

/// USB audio interfaces and displays often have no mute control at all.
#[cfg(target_os = "macos")]
fn has_output_mute_control(device_id: AudioObjectId) -> bool {
    let address = output_mute_address();
    let mut settable: u8 = 0;
    let status = unsafe { AudioObjectIsPropertySettable(device_id, &address, &mut settable) };
    status == 0 && settable != 0
}

#[cfg(target_os = "macos")]
fn get_output_muted(device_id: AudioObjectId) -> Result<bool, String> {
    let address = output_mute_address();
    let mut muted: u32 = 0;
    let mut data_size = size_of_val_u32(&muted)?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            (&mut muted as *mut u32).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Getting output mute", status));
    }

    Ok(muted != 0)
}

#[cfg(target_os = "macos")]
fn set_output_muted(device_id: AudioObjectId, muted: bool) -> Result<(), String> {
    let address = output_mute_address();
    let value = u32::from(muted);

    let status = unsafe {
        AudioObjectSetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            size_of_val_u32(&value)?,
            (&value as *const u32).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Setting output mute", status));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn output_volume_address() -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector: AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

#[cfg(target_os = "macos")]
fn is_output_volume_settable(device_id: AudioObjectId) -> Result<bool, String> {
    let address = output_volume_address();
    let mut settable: u8 = 0;

    let status = unsafe { AudioObjectIsPropertySettable(device_id, &address, &mut settable) };

    if status != 0 {
        return Err(core_audio_error("Checking output volume", status));
    }

    Ok(settable != 0)
}

#[cfg(target_os = "macos")]
fn get_output_volume(device_id: AudioObjectId) -> Result<f32, String> {
    let address = output_volume_address();
    let mut volume: f32 = 0.0;
    let mut data_size = size_of_val_u32(&volume)?;

    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            (&mut volume as *mut f32).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Getting output volume", status));
    }

    Ok(volume)
}

#[cfg(target_os = "macos")]
fn set_output_volume(device_id: AudioObjectId, volume: f32) -> Result<(), String> {
    let address = output_volume_address();
    let value = volume.clamp(0.0, 1.0);

    let status = unsafe {
        AudioObjectSetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            size_of_val_u32(&value)?,
            (&value as *const f32).cast(),
        )
    };

    if status != 0 {
        return Err(core_audio_error("Setting output volume", status));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn has_output_volume_control(device_id: AudioObjectId) -> bool {
    is_output_volume_settable(device_id).unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn get_u32_property(
    device_id: AudioObjectId,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> Option<u32> {
    let address = AudioObjectPropertyAddress {
        selector,
        scope,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut value: u32 = 0;
    let mut data_size = size_of_val_u32(&value).ok()?;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            (&mut value as *mut u32).cast(),
        )
    };
    (status == 0).then_some(value)
}

#[cfg(target_os = "macos")]
fn property_data_size(
    object_id: AudioObjectId,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> u32 {
    let address = AudioObjectPropertyAddress {
        selector,
        scope,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut data_size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(object_id, &address, 0, std::ptr::null(), &mut data_size)
    };
    if status == 0 { data_size } else { 0 }
}

#[cfg(target_os = "macos")]
fn get_devices() -> Vec<AudioObjectId> {
    let count = property_data_size(
        AUDIO_OBJECT_SYSTEM_OBJECT,
        AUDIO_HARDWARE_PROPERTY_DEVICES,
        AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
    ) as usize
        / std::mem::size_of::<AudioObjectId>();
    let mut devices = vec![AUDIO_OBJECT_UNKNOWN; count];
    let address = global_address(AUDIO_HARDWARE_PROPERTY_DEVICES);
    let Ok(mut data_size) = u32::try_from(std::mem::size_of_val(devices.as_slice())) else {
        return Vec::new();
    };
    let status = unsafe {
        AudioObjectGetPropertyData(
            AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            std::ptr::null(),
            &mut data_size,
            devices.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    devices.truncate(data_size as usize / std::mem::size_of::<AudioObjectId>());
    devices
}

#[cfg(target_os = "macos")]
fn is_bluetooth(device_id: AudioObjectId) -> bool {
    get_u32_property(
        device_id,
        AUDIO_DEVICE_PROPERTY_TRANSPORT_TYPE,
        AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
    )
    .is_some_and(|transport| {
        transport == AUDIO_DEVICE_TRANSPORT_TYPE_BLUETOOTH
            || transport == AUDIO_DEVICE_TRANSPORT_TYPE_BLUETOOTH_LE
    })
}

/// Whether the output is a Bluetooth headset whose microphone is recording.
/// It then plays in its call profile and switches back when the recording
/// ends, while a tap would still be fading out.
#[cfg(target_os = "macos")]
fn is_bluetooth_headset_recording(device_id: AudioObjectId) -> bool {
    if !is_bluetooth(device_id) {
        return false;
    }
    let Ok(uid) = get_device_uid(device_id) else {
        return false;
    };
    // The microphone is a separate device whose UID starts with the same
    // Bluetooth address.
    let address = uid.split(':').next().unwrap_or(&uid).to_string();
    get_devices().into_iter().any(|other| {
        other != device_id
            && is_bluetooth(other)
            && property_data_size(
                other,
                AUDIO_DEVICE_PROPERTY_STREAMS,
                AUDIO_DEVICE_PROPERTY_SCOPE_INPUT,
            ) > 0
            && get_device_uid(other).is_ok_and(|other_uid| other_uid.starts_with(&address))
            && get_u32_property(
                other,
                AUDIO_DEVICE_PROPERTY_DEVICE_IS_RUNNING_SOMEWHERE,
                AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            )
            .is_some_and(|running| running != 0)
    })
}

/// Changes the output's own volume, and other apps' audio on it through taps.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct CoreAudioOutput {
    /// Taps by device UID, with the ID the device last had.
    taps: std::cell::RefCell<
        std::collections::HashMap<String, (AudioObjectId, muffle::TapEffect, tap::OutputTap)>,
    >,
}

#[cfg(target_os = "macos")]
impl CoreAudioOutput {
    /// Without the System Audio Recording permission the tap delivers silence,
    /// which would mute instead of muffle.
    fn start_muffle_tap(device: AudioObjectId, uid: &str) -> Result<tap::OutputTap, String> {
        // For the next recording, in case it changed in System Settings.
        tap::refresh_permission();
        match tap::audio_capture_permission() {
            tap::Permission::Granted => {}
            tap::Permission::Denied => {
                return Err("System Audio Recording permission is off".to_string());
            }
            tap::Permission::Unknown => {
                return Err("System Audio Recording permission cannot be checked".to_string());
            }
            tap::Permission::Undetermined => {
                // Ask now; the filter works from the next recording on.
                tap::request_permission(uid);
                return Err("System Audio Recording permission was not asked yet".to_string());
            }
        }
        if is_bluetooth_headset_recording(device) {
            return Err("The output is a Bluetooth headset that is recording".to_string());
        }
        tap::OutputTap::start(uid, 1.0)
    }
}

#[cfg(target_os = "macos")]
impl muffle::Output for CoreAudioOutput {
    fn default_output_device(&self) -> Result<AudioObjectId, String> {
        get_default_output_device()
    }

    fn device_uid(&self, device: AudioObjectId) -> Option<String> {
        get_device_uid(device).ok()
    }

    fn find_device_by_uid(&self, uid: &str) -> Option<AudioObjectId> {
        find_device_by_uid(uid)
    }

    fn adjustable_volume(&self, device: AudioObjectId) -> Result<f32, String> {
        if !has_output_volume_control(device) {
            return Err("Output volume is fixed".to_string());
        }
        get_output_volume(device)
    }

    fn set_volume(&self, device: AudioObjectId, volume: f32) -> Result<(), String> {
        set_output_volume(device, volume)
    }

    fn start_tap(&self, device: AudioObjectId, effect: muffle::TapEffect) -> Result<(), String> {
        if !tap::is_supported() {
            return Err("Output taps need macOS 14.2 or later".to_string());
        }
        let uid = get_device_uid(device)?;
        let output_tap = match effect {
            muffle::TapEffect::Muffle => Self::start_muffle_tap(device, &uid)?,
            // Muting needs neither playback nor the permission: the tap holds
            // the audio back either way.
            muffle::TapEffect::Silence => tap::OutputTap::start(&uid, 0.0)?,
        };
        log::info!(target: "media", "output_tap_started device_id={device} effect={effect:?}");
        self.taps
            .borrow_mut()
            .insert(uid, (device, effect, output_tap));
        Ok(())
    }

    fn set_tap(&self, device: AudioObjectId, amount: f32) -> Result<(), String> {
        let mut taps = self.taps.borrow_mut();
        let uid = get_device_uid(device)?;
        let Some((tapped, effect, output_tap)) = taps.get_mut(&uid) else {
            return Err("The output has no tap".to_string());
        };
        *tapped = device;
        match effect {
            muffle::TapEffect::Muffle => output_tap.set_muffle(amount),
            muffle::TapEffect::Silence => output_tap.set_volume(1.0 - amount),
        }
        Ok(())
    }

    fn stop_tap(&self, device: AudioObjectId) {
        let uid = get_device_uid(device).ok();
        let mut taps = self.taps.borrow_mut();
        let before = taps.len();
        taps.retain(|tapped_uid, (tapped, _, _)| {
            *tapped != device && uid.as_deref() != Some(tapped_uid.as_str())
        });
        if taps.len() < before {
            log::info!(target: "media", "output_tap_stopped device_id={device}");
        }
    }
}

/// Unmutes devices that were unplugged while muted for a recording, now that
/// they are connected again.
#[cfg(target_os = "macos")]
fn unmute_returned_devices() {
    let Ok(mut state) = MEDIA_MUTE_STATE.lock() else {
        return;
    };
    state.pending_unmute_uids.retain(|uid| {
        let Some(device_id) = find_device_by_uid(uid) else {
            return true;
        };
        match set_output_muted(device_id, false) {
            Ok(()) => {
                log::info!(target: "media", "returned_output_unmuted device_id={device_id}");
                false
            }
            Err(err) => {
                log::warn!(target: "media", "returned_output_unmute_failed device_id={device_id} error={err}");
                true
            }
        }
    });
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn on_audio_devices_changed(
    _in_object_id: AudioObjectId,
    _in_number_addresses: u32,
    _in_addresses: *const AudioObjectPropertyAddress,
    _in_client_data: *mut std::ffi::c_void,
) -> OsStatus {
    // Runs on a CoreAudio notification thread; do the work elsewhere.
    std::thread::spawn(|| {
        unmute_returned_devices();
        muffle::devices_changed();
        if let Some(events) = EVENTS.get() {
            events.emit(crate::backend::AppEvent::AudioDevicesChanged);
        }
    });
    0
}

/// Watches for audio devices being connected or disconnected and for default
/// output changes. Emits `audio-devices-changed` so the interface can list
/// the devices again and reopen audio output on the new one.
#[cfg(target_os = "macos")]
pub fn watch_audio_devices(events: &crate::backend::EventSender) {
    EVENTS.set(events.clone()).ok();

    for selector in [
        AUDIO_HARDWARE_PROPERTY_DEVICES,
        AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
    ] {
        let address = global_address(selector);
        let status = unsafe {
            AudioObjectAddPropertyListener(
                AUDIO_OBJECT_SYSTEM_OBJECT,
                &address,
                on_audio_devices_changed,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            log::warn!(
                target: "media",
                "{}",
                core_audio_error("Adding audio device listener", status)
            );
        }
    }
}

/// Changes a sink's volume through the sound server. Sinks have no taps:
/// muffling turns the volume down instead of filtering.
#[cfg(target_os = "linux")]
#[derive(Default)]
struct PulseOutput {
    /// Each sink's channel volumes relative to its loudest channel, as they
    /// were before the worker changed them, so a fade keeps the balance.
    balances: std::cell::RefCell<std::collections::HashMap<u32, Vec<f32>>>,
}

#[cfg(target_os = "linux")]
fn default_sink(connection: &mut pulse::Connection) -> Result<pulseaudio::protocol::SinkInfo, String> {
    let name = connection
        .server_info()?
        .default_sink_name
        .ok_or("There is no default output")?;
    connection
        .sinks()?
        .into_iter()
        .find(|sink| sink.name == name)
        .ok_or_else(|| "The default output is gone".to_string())
}

#[cfg(target_os = "linux")]
fn find_sink_by_name(name: &str) -> Option<u32> {
    pulse::with_connection(|connection| connection.sinks())
        .ok()?
        .into_iter()
        .find(|sink| sink.name.to_bytes() == name.as_bytes())
        .map(|sink| sink.index)
}

#[cfg(target_os = "linux")]
impl muffle::Output for PulseOutput {
    fn default_output_device(&self) -> Result<u32, String> {
        pulse::with_connection(|connection| default_sink(connection).map(|sink| sink.index))
    }

    fn device_uid(&self, device: u32) -> Option<String> {
        pulse::with_connection(|connection| connection.sink(device))
            .ok()
            .map(|sink| sink.name.to_string_lossy().into_owned())
    }

    fn find_device_by_uid(&self, uid: &str) -> Option<u32> {
        find_sink_by_name(uid)
    }

    fn adjustable_volume(&self, device: u32) -> Result<f32, String> {
        use pulseaudio::protocol::Volume;

        let sink = pulse::with_connection(|connection| connection.sink(device))?;
        let channels: Vec<f32> = sink
            .cvolume
            .channels()
            .iter()
            .map(|volume| volume.as_u32() as f32 / Volume::NORM.as_u32() as f32)
            .collect();
        let loudest = channels.iter().copied().fold(0.0, f32::max);
        let balance = if loudest > 0.0 {
            channels.iter().map(|volume| volume / loudest).collect()
        } else {
            vec![1.0; channels.len()]
        };
        self.balances.borrow_mut().insert(device, balance);
        Ok(loudest)
    }

    fn set_volume(&self, device: u32, volume: f32) -> Result<(), String> {
        use pulseaudio::protocol::{ChannelVolume, Volume};

        let balance = self.balances.borrow().get(&device).cloned();
        let balance = match balance {
            Some(balance) => balance,
            None => {
                self.adjustable_volume(device)?;
                self.balances.borrow().get(&device).cloned().unwrap_or_default()
            }
        };
        let mut channels = ChannelVolume::empty();
        for share in &balance {
            let raw = (volume.max(0.0) * share * Volume::NORM.as_u32() as f32).round();
            channels.push(Volume::from_u32_clamped(raw as u32));
        }
        pulse::with_connection(|connection| connection.set_sink_volume(device, channels.clone()))
    }

    fn start_tap(&self, _device: u32, _effect: muffle::TapEffect) -> Result<(), String> {
        Err("Outputs cannot be tapped on Linux".to_string())
    }

    fn set_tap(&self, _device: u32, _amount: f32) -> Result<(), String> {
        Err("Outputs cannot be tapped on Linux".to_string())
    }

    fn stop_tap(&self, _device: u32) {}
}

/// Unmutes sinks that were unplugged while muted for a recording, now that
/// they are connected again.
#[cfg(target_os = "linux")]
fn unmute_returned_devices() {
    let Ok(mut state) = MEDIA_MUTE_STATE.lock() else {
        return;
    };
    state.pending_unmute_sinks.retain(|name| {
        let Some(index) = find_sink_by_name(name) else {
            return true;
        };
        match pulse::with_connection(|connection| connection.set_sink_mute(index, false)) {
            Ok(()) => {
                log::info!(target: "media", "returned_output_unmuted device_id={index}");
                false
            }
            Err(err) => {
                log::warn!(target: "media", "returned_output_unmute_failed device_id={index} error={err}");
                true
            }
        }
    });
}

/// Watches for outputs and microphones being connected or disconnected and
/// for changes of the default ones. Emits `audio-devices-changed` so the
/// interface can list the devices again and reopen audio output.
#[cfg(target_os = "linux")]
pub fn watch_audio_devices(events: &crate::backend::EventSender) {
    if !pulse::is_available() {
        log::warn!(target: "media", "sound_server_unavailable");
        return;
    }
    let events = events.clone();
    pulse::watch_devices(move || {
        unmute_returned_devices();
        muffle::devices_changed();
        events.emit(crate::backend::AppEvent::AudioDevicesChanged);
    });
}

pub fn mute_media_for_recording() -> Result<(), String> {
    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if state.active {
        return Ok(());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    mute_default_output(&mut state)?;

    state.active = true;
    Ok(())
}

/// Logs its failures itself, because the interface does not wait for them.
#[cfg(target_os = "linux")]
fn mute_default_output(state: &mut MediaMuteState) -> Result<(), String> {
    let result = pulse::with_connection(|connection| {
        let sink = default_sink(connection)?;
        if !sink.muted {
            connection.set_sink_mute(sink.index, true)?;
            state.muted_sink = Some(sink.name.to_string_lossy().into_owned());
        }
        Ok(())
    });
    if let Err(err) = &result {
        log::warn!(target: "media", "mute_failed error={err}");
    }
    result
}

/// Logs its failures itself, because the interface does not wait for them.
#[cfg(target_os = "macos")]
fn mute_default_output(state: &mut MediaMuteState) -> Result<(), String> {
    let device_id = get_default_output_device().inspect_err(|err| {
        log::warn!(target: "media", "mute_failed error={err}");
    })?;
    if !has_output_mute_control(device_id) {
        // Turn other apps' audio all the way down instead.
        muffle::silence();
        return Ok(());
    }
    let result = get_output_muted(device_id).and_then(|already_muted| {
        if !already_muted {
            set_output_muted(device_id, true)?;
            state.muted_device = Some(MutedDevice {
                id: device_id,
                uid: get_device_uid(device_id).ok(),
            });
        }
        Ok(())
    });
    if let Err(err) = &result {
        log::warn!(target: "media", "mute_failed device_id={device_id} error={err}");
    }
    result
}

/// Muffles other apps' audio for the recording instead of muting it, or turns
/// the volume down where the muffle filter cannot run. Returns at once; the
/// fade runs on a background thread, so it never delays the recording.
pub fn muffle_media_for_recording() {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    muffle::muffle();
}

/// Runs instead of the app when it was started only to ask for the System
/// Audio Recording permission. Returns whether it did.
pub fn run_permission_request_if_asked() -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut args = std::env::args().skip(1);
        match args.next().as_deref() {
            Some(tap::REQUEST_PERMISSION_FLAG) => {
                if let Some(device_uid) = args.next() {
                    tap::hold_permission_request(&device_uid);
                }
                return true;
            }
            Some(tap::CHECK_PERMISSION_FLAG) => tap::exit_with_permission(),
            _ => {}
        }
    }
    false
}

/// Asks for the System Audio Recording permission that the muffle filter
/// needs, if it was not asked yet. Returns at once.
pub fn request_muffle_permission() {
    if let Err(err) = try_request_muffle_permission() {
        log::warn!(target: "media", "audio_capture_permission_request_failed error={err}");
    }
}

/// [`request_muffle_permission`], telling why nothing could be asked.
pub fn try_request_muffle_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if tap::is_supported() && tap::audio_capture_permission() == tap::Permission::Undetermined {
        let uid = get_default_output_device().and_then(get_device_uid)?;
        tap::request_permission(&uid);
    }
    Ok(())
}

/// What macOS says about System Audio Recording, or `None` where the muffle
/// filter cannot run or macOS does not tell.
pub fn muffle_permission() -> Option<crate::backend::PermissionState> {
    #[cfg(target_os = "macos")]
    if tap::is_supported() {
        use crate::backend::PermissionState;

        return match tap::audio_capture_permission() {
            tap::Permission::Granted => Some(PermissionState::Granted),
            tap::Permission::Denied => Some(PermissionState::Denied),
            tap::Permission::Undetermined => Some(PermissionState::Prompt),
            tap::Permission::Unknown => None,
        };
    }
    None
}

/// Puts output back after a recording, whether it was muted or muffled.
pub fn restore_media_after_recording() -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    muffle::restore();

    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if !state.active {
        return Ok(());
    }
    state.active = false;

    // Unmute the device that was muted, even if the default output has moved
    // to another device since (e.g. a headset was plugged in or pulled out).
    #[cfg(target_os = "macos")]
    {
        if let Some(device) = state.muted_device.take() {
            // A device unplugged and reconnected mid-recording can come back
            // under a new ID, so look it up by UID first.
            let device_id = device
                .uid
                .as_deref()
                .and_then(find_device_by_uid)
                .unwrap_or(device.id);
            if let Err(err) = set_output_muted(device_id, false) {
                log::warn!(
                    target: "media",
                    "restore_output_mute_failed device_id={device_id} error={err}"
                );
                state.pending_unmute_uids.extend(device.uid);
            }
        }
    }

    // Unmute the sink that was muted, even if the default output has moved
    // since; it keeps its name when it is unplugged and plugged back in.
    #[cfg(target_os = "linux")]
    if let Some(name) = state.muted_sink.take() {
        let unmuted = find_sink_by_name(&name).ok_or_else(|| "The output is gone".to_string()).and_then(
            |index| pulse::with_connection(|connection| connection.set_sink_mute(index, false)),
        );
        if let Err(err) = unmuted {
            log::warn!(target: "media", "restore_output_mute_failed sink={name} error={err}");
            state.pending_unmute_sinks.push(name);
        }
    }

    Ok(())
}

/// Puts output back if the app quits mid-recording.
pub fn restore_media_on_exit() {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    muffle::restore_now();
    if let Err(err) = restore_media_after_recording() {
        log::warn!(target: "media", "exit_restore_failed error={err}");
    }
}
