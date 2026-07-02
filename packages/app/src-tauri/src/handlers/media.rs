use std::sync::{LazyLock, Mutex};

#[derive(Default)]
struct MediaMuteState {
    active: bool,
    was_muted: bool,
}

static MEDIA_MUTE_STATE: LazyLock<Mutex<MediaMuteState>> =
    LazyLock::new(|| Mutex::new(MediaMuteState::default()));

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

    fn AudioObjectSetPropertyData(
        in_object_id: AudioObjectId,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const std::ffi::c_void,
        in_data_size: u32,
        in_data: *const std::ffi::c_void,
    ) -> OsStatus;
}

#[cfg(target_os = "macos")]
const AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectId = 1;

#[cfg(target_os = "macos")]
const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"dOut");

#[cfg(target_os = "macos")]
const AUDIO_DEVICE_PROPERTY_MUTE: AudioObjectPropertySelector = u32::from_be_bytes(*b"mute");

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
fn get_default_output_device() -> Result<AudioObjectId, String> {
    let address = AudioObjectPropertyAddress {
        selector: AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
        scope: AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
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
fn size_of_val_u32<T>(value: &T) -> Result<u32, String> {
    u32::try_from(std::mem::size_of_val(value))
        .map_err(|_| "CoreAudio data size overflow".to_string())
}

#[cfg(target_os = "macos")]
fn get_output_muted() -> Result<bool, String> {
    let device_id = get_default_output_device()?;
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
fn set_output_muted(muted: bool) -> Result<(), String> {
    let device_id = get_default_output_device()?;
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

#[tauri::command]
pub fn mute_media_for_recording() -> Result<(), String> {
    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if state.active {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        state.was_muted = get_output_muted()?;
        if !state.was_muted {
            set_output_muted(true)?;
        }
        state.active = true;
    }

    #[cfg(not(target_os = "macos"))]
    {
        state.active = true;
        state.was_muted = false;
    }

    Ok(())
}

#[tauri::command]
pub fn restore_media_after_recording() -> Result<(), String> {
    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if !state.active {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if !state.was_muted {
            set_output_muted(false)?;
        }
    }

    state.active = false;
    state.was_muted = false;
    Ok(())
}
