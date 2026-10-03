use std::sync::{LazyLock, Mutex};

#[cfg(target_os = "macos")]
mod muffle;

#[derive(Default)]
struct MediaMuteState {
    active: bool,
    /// Output device muted for the current recording. `None` when it was
    /// already muted, so restoring leaves it alone.
    #[cfg(target_os = "macos")]
    muted_device: Option<MutedDevice>,
    /// UIDs of devices that were unplugged while muted. macOS can restore a
    /// device's saved mute state when it reconnects, so they are unmuted once
    /// they are back.
    #[cfg(target_os = "macos")]
    pending_unmute_uids: Vec<String>,
}

#[cfg(target_os = "macos")]
struct MutedDevice {
    id: AudioObjectId,
    uid: Option<String>,
}

static MEDIA_MUTE_STATE: LazyLock<Mutex<MediaMuteState>> =
    LazyLock::new(|| Mutex::new(MediaMuteState::default()));

#[cfg(target_os = "macos")]
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

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

/// The volume behind the system volume slider, from 0 to 1. Unlike a device's
/// per-channel volumes it exists on any output with adjustable volume.
#[cfg(target_os = "macos")]
const AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"vmvc");

#[cfg(target_os = "macos")]
const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope = u32::from_be_bytes(*b"glob");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: AudioObjectPropertyScope = u32::from_be_bytes(*b"outp");

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
fn get_output_muted(device_id: AudioObjectId) -> Result<bool, String> {
    let address = AudioObjectPropertyAddress {
        selector: AUDIO_DEVICE_PROPERTY_MUTE,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
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
    let address = AudioObjectPropertyAddress {
        selector: AUDIO_DEVICE_PROPERTY_MUTE,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
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
struct CoreAudioVolume;

#[cfg(target_os = "macos")]
impl muffle::OutputVolume for CoreAudioVolume {
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
        if !is_output_volume_settable(device)? {
            return Err("Output volume is fixed".to_string());
        }
        get_output_volume(device)
    }

    fn set_volume(&self, device: AudioObjectId, volume: f32) -> Result<(), String> {
        set_output_volume(device, volume)
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
    use tauri::Emitter;

    // Runs on a CoreAudio notification thread; do the work elsewhere.
    std::thread::spawn(|| {
        unmute_returned_devices();
        muffle::devices_changed();
        if let Some(handle) = APP_HANDLE.get() {
            let _ = handle.emit("audio-devices-changed", ());
        }
    });
    0
}

/// Watches for audio devices being connected or disconnected and for default
/// output changes. Emits `audio-devices-changed` so webviews can reopen audio
/// output on the new device.
#[cfg(target_os = "macos")]
pub fn watch_audio_devices(app_handle: &tauri::AppHandle) {
    APP_HANDLE.set(app_handle.clone()).ok();

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

#[tauri::command]
pub fn mute_media_for_recording() -> Result<(), String> {
    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if state.active {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let device_id = get_default_output_device()?;
        if !get_output_muted(device_id)? {
            set_output_muted(device_id, true)?;
            state.muted_device = Some(MutedDevice {
                id: device_id,
                uid: get_device_uid(device_id).ok(),
            });
        }
    }

    state.active = true;
    Ok(())
}

/// Turns system output down for the recording instead of muting it. Returns
/// at once; the fade runs on a background thread, so it never delays the
/// recording.
#[tauri::command]
pub fn muffle_media_for_recording() {
    #[cfg(target_os = "macos")]
    muffle::muffle();
}

/// Puts output back after a recording, whether it was muted or muffled.
#[tauri::command]
pub fn restore_media_after_recording() -> Result<(), String> {
    #[cfg(target_os = "macos")]
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

    Ok(())
}

/// Puts output back if the app quits mid-recording.
pub fn restore_media_on_exit() {
    #[cfg(target_os = "macos")]
    muffle::restore_now();
    if let Err(err) = restore_media_after_recording() {
        log::warn!(target: "media", "exit_restore_failed error={err}");
    }
}
