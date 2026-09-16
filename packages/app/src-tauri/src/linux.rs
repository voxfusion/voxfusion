//! Hyprland owns Wayland shortcuts. Keep bindings in the running compositor only;
//! never rewrite the user's Omarchy configuration. A private Unix socket forwards
//! presses to the existing process before GTK starts, preserving keyboard focus.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    process::Command,
    sync::Mutex,
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Clone)]
struct Binding {
    mods: String,
    mask: u64,
    key: String,
    release: bool,
}
#[derive(Default)]
pub struct Shortcuts(Mutex<HashMap<String, Binding>>);

#[derive(Clone, Serialize, Deserialize)]
pub struct ShortcutEvent {
    hotkey: String,
    state: String,
}

fn socket_path(identifier: &str) -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|dir| PathBuf::from(dir).join(format!("{identifier}.control.sock")))
        .ok_or_else(|| {
            "XDG_RUNTIME_DIR is missing; launch VoxFusion from your desktop session.".into()
        })
}

/// Returns true only for a control invocation. Ordinary launches continue into Tauri.
pub fn handle_cli() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let event = match args.first().map(String::as_str) {
        Some("--toggle") => Some(("toggle".to_string(), "Pressed".to_string())),
        Some("--start") => Some(("hold".to_string(), "Pressed".to_string())),
        Some("--stop") => Some(("hold".to_string(), "Released".to_string())),
        Some("--cancel") => Some(("Escape".to_string(), "Pressed".to_string())),
        Some("--shortcut") if args.len() == 5 && args[3] == "--control-socket" => {
            Some((args[1].clone(), args[2].clone()))
        }
        _ => None,
    };
    let Some((hotkey, state)) = event else {
        return false;
    };
    let result = (|| -> Result<(), String> {
        let path = if args.first().is_some_and(|arg| arg == "--shortcut") {
            PathBuf::from(&args[4])
        } else {
            socket_path("io.voxfusion.app")?
        };
        let mut stream =
            UnixStream::connect(path).map_err(|e| format!("Start VoxFusion first: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let payload =
            serde_json::to_string(&ShortcutEvent { hotkey, state }).map_err(|e| e.to_string())?;
        writeln!(stream, "{payload}").map_err(|e| e.to_string())?;
        let mut reply = String::new();
        stream
            .read_to_string(&mut reply)
            .map_err(|e| e.to_string())?;
        if reply.trim() != "ok" {
            return Err(reply);
        }
        Ok(())
    })();
    if let Err(err) = result {
        eprintln!("VoxFusion: {err}");
        std::process::exit(1);
    }
    true
}

fn hyprctl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("hyprctl")
        .args(args)
        .output()
        .map_err(|e| format!("Hyprland: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || (args.first() == Some(&"keyword") && stdout != "ok") {
        return Err(format!(
            "Hyprland: {stdout} {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(stdout)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_hotkey(hotkey: &str, release: bool) -> Result<Binding, String> {
    let parts: Vec<_> = hotkey.split('+').collect();
    let mut mods = Vec::new();
    let mut mask = 0;
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        let (name, bit) = match *modifier {
            "Control" | "Ctrl" | "CommandOrControl" | "CommandOrCtrl" | "CmdOrCtrl"
            | "CmdOrControl" => ("CTRL", 4),
            "Alt" | "Option" => ("ALT", 8),
            "Shift" => ("SHIFT", 1),
            "Super" | "Command" | "Cmd" => ("SUPER", 64),
            _ => return Err(format!("Unsupported modifier: {modifier}")),
        };
        if mask & bit == 0 {
            mods.push(name);
            mask |= bit;
        }
    }
    let key = match *parts.last().unwrap_or(&"") {
        "Space" => "space",
        "Enter" => "Return",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "," => "comma",
        "." => "period",
        ";" => "semicolon",
        "'" => "apostrophe",
        "/" => "slash",
        "\\" => "backslash",
        "[" => "bracketleft",
        "]" => "bracketright",
        "`" => "grave",
        "-" => "minus",
        "=" => "equal",
        key if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) => key,
        key if matches!(
            key,
            "Escape" | "Tab" | "Backspace" | "Delete" | "Home" | "End" | "PageUp" | "PageDown"
        ) =>
        {
            key
        }
        key if key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n)) =>
        {
            key
        }
        _ => {
            return Err(
                "Use modifiers and a letter, number, function key or navigation key.".into(),
            );
        }
    };
    if mask == 0 && key != "Escape" {
        return Err("A shortcut needs at least one modifier.".into());
    }
    Ok(Binding {
        mods: mods.join(" "),
        mask,
        key: key.to_string(),
        release,
    })
}

fn existing_binds() -> Result<Vec<serde_json::Value>, String> {
    serde_json::from_str(&hyprctl(&["-j", "binds"])?).map_err(|e| e.to_string())
}

fn matching<'a>(binds: &'a [serde_json::Value], binding: &Binding) -> Vec<&'a serde_json::Value> {
    binds
        .iter()
        .filter(|b| {
            b["modmask"].as_u64() == Some(binding.mask)
                && b["key"]
                    .as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case(&binding.key))
        })
        .collect()
}

fn description(app: &tauri::AppHandle) -> String {
    format!("VoxFusion {}", app.config().identifier)
}

fn install_binding(app: &tauri::AppHandle, hotkey: &str, binding: &Binding) -> Result<(), String> {
    let binds = existing_binds()?;
    let matches = matching(&binds, binding);
    let owner = description(app);
    if matches
        .iter()
        .any(|b| b["description"].as_str() != Some(owner.as_str()))
    {
        return Err(format!(
            "{hotkey} is already used by Hyprland. Choose another shortcut in Settings."
        ));
    }
    if !matches.is_empty() {
        return Ok(());
    }
    let executable = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe().map_err(|e| e.to_string())?);
    let socket = socket_path(&app.config().identifier)?;
    for state in if binding.release {
        vec!["Pressed", "Released"]
    } else {
        vec!["Pressed"]
    } {
        let command = format!(
            "{} --shortcut {} {} --control-socket {}",
            shell_quote(&executable.to_string_lossy()),
            shell_quote(hotkey),
            state,
            shell_quote(&socket.to_string_lossy())
        );
        let rule = format!(
            "{}, {}, {}, exec, {}",
            binding.mods, binding.key, owner, command
        );
        hyprctl(&[
            "keyword",
            if state == "Released" {
                "bindrd"
            } else {
                "bindd"
            },
            &rule,
        ])?;
    }
    Ok(())
}

pub fn register_shortcut(
    app: &tauri::AppHandle,
    hotkey: &str,
    release: bool,
) -> Result<(), String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Err("Automatic Wayland shortcuts require Hyprland. Configure your compositor to run voxfusion-app --toggle, --start and --stop.".into());
    }
    let binding = parse_hotkey(hotkey, release)?;
    let state = app.state::<Shortcuts>();
    let mut bindings = state.0.lock().map_err(|e| e.to_string())?;
    install_binding(app, hotkey, &binding)?;
    bindings.insert(hotkey.into(), binding);
    Ok(())
}

pub fn unregister_shortcut(app: &tauri::AppHandle, hotkey: &str) -> Result<(), String> {
    let state = app.state::<Shortcuts>();
    let mut bindings = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(binding) = bindings.remove(hotkey) {
        let binds = existing_binds()?;
        let matches = matching(&binds, &binding);
        let owner = description(app);
        // Hyprland's unbind removes ALL bindings for this chord. Never remove a
        // user binding added while VoxFusion was running.
        if !matches.is_empty()
            && matches
                .iter()
                .all(|b| b["description"].as_str() == Some(owner.as_str()))
        {
            hyprctl(&[
                "keyword",
                "unbind",
                &format!("{}, {}", binding.mods, binding.key),
            ])?;
        }
    }
    Ok(())
}

const OVERLAY_TITLE: &str = "VoxFusion Voice Control";

fn configure_overlay() -> Result<(), String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Ok(());
    }
    let version: serde_json::Value =
        serde_json::from_str(&hyprctl(&["-j", "version"])?).map_err(|e| e.to_string())?;
    let minor = version["tag"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
        .split('.')
        .nth(1)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(52);
    if minor >= 53 {
        for effect in [
            "float on",
            "no_focus on",
            "no_border on",
            "no_shadow on",
            "no_blur on",
            "pin on",
            "move monitor_w-window_w-20 monitor_h-window_h-20",
        ] {
            hyprctl(&[
                "keyword",
                "windowrule",
                &format!("match:title ^{OVERLAY_TITLE}$, {effect}"),
            ])?;
        }
    } else {
        for effect in [
            "float",
            "nofocus",
            "noborder",
            "noshadow",
            "noblur",
            "pin",
            "move onscreen 100%-w-20 100%-w-20",
        ] {
            hyprctl(&[
                "keyword",
                "windowrulev2",
                &format!("{effect},title:^({OVERLAY_TITLE})$"),
            ])?;
        }
    }
    Ok(())
}

pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(Shortcuts::default());
    if let Err(error) = configure_overlay() {
        log::warn!("Overlay configuration: {error}");
    }
    let path = socket_path(&app.config().identifier)?;
    // Single-instance plugin has already rejected a second GUI process.
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let handle = app.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let mut line = String::new();
            if BufReader::new((&stream).take(4096))
                .read_line(&mut line)
                .is_ok()
            {
                if let Ok(event) = serde_json::from_str::<ShortcutEvent>(&line) {
                    if matches!(event.state.as_str(), "Pressed" | "Released") {
                        let result = handle.emit_to("voice-control", "linux-shortcut", event);
                        let _ = stream.write_all(if result.is_ok() {
                            b"ok\n"
                        } else {
                            b"Voice control unavailable\n"
                        });
                    }
                }
            }
        }
    });
    // A config reload discards runtime binds. Reinstall only our missing binds;
    // collisions added by the user are reported, never overwritten.
    if let (Some(runtime), Some(instance)) = (
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE"),
    ) {
        let events = PathBuf::from(runtime)
            .join("hypr")
            .join(instance)
            .join(".socket2.sock");
        let handle = app.clone();
        std::thread::spawn(move || {
            if let Ok(stream) = UnixStream::connect(events) {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if line.starts_with("configreloaded>>") {
                        if let Err(error) = configure_overlay() {
                            log::warn!("Overlay restore: {error}");
                        }
                        let state = handle.state::<Shortcuts>();
                        if let Ok(bindings) = state.0.lock() {
                            for (hotkey, binding) in bindings.iter() {
                                if let Err(err) = install_binding(&handle, hotkey, binding) {
                                    log::error!("Shortcut restore failed: {err}");
                                    let _ = handle.emit("shortcut-error", err);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
    Ok(())
}

pub fn cleanup(app: &tauri::AppHandle) {
    if let Err(error) = crate::handlers::media::restore_media_after_recording() {
        log::warn!("Could not restore audio on exit: {error}");
    }
    let keys: Vec<_> = app
        .state::<Shortcuts>()
        .0
        .lock()
        .map(|b| b.keys().cloned().collect())
        .unwrap_or_default();
    for key in keys {
        let _ = unregister_shortcut(app, &key);
    }
    if let Ok(path) = socket_path(&app.config().identifier) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_mapping_and_input_validation() {
        let binding = parse_hotkey("Control+Alt+V", true).unwrap();
        assert_eq!(binding.mask, 12);
        assert_eq!(binding.mods, "CTRL ALT");
        assert!(binding.release);
        assert_eq!(parse_hotkey("Control+;", false).unwrap().key, "semicolon");
        for bad in [
            "RightCommand",
            "Alt",
            "Control+exec,evil",
            "Control+V\nexec=evil",
            "V",
            "Control+F99",
        ] {
            assert!(parse_hotkey(bad, false).is_err(), "accepted {bad}");
        }
    }
    #[test]
    fn quotes_executable_paths_without_expansion() {
        assert_eq!(
            shell_quote("/tmp/a b'$(touch nope)"),
            "'/tmp/a b'\\''$(touch nope)'"
        );
    }
}
