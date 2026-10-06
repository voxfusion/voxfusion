//! Apps on a Linux desktop: the installed ones, from their desktop entries,
//! and the one in front, from the X11 window manager.
//!
//! An app is identified by its desktop file ID (`org.gnome.TextEditor`,
//! `firefox_firefox`), which stands in for the macOS bundle identifier.

use base64::Engine as _;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::backend::{FrontmostApp, InstalledApp};

/// How long the desktop entries read for the frontmost app are reused.
const ENTRIES_LIFETIME: Duration = Duration::from_secs(30);

/// The icon size looked for, in pixels; the list shows icons at 32 points.
const ICON_SIZE: u32 = 64;

/// What an app's desktop entry says about it.
#[derive(Debug, Clone, PartialEq)]
struct DesktopEntry {
    id: String,
    name: String,
    icon: Option<String>,
    /// The file name of the program it runs.
    program: Option<String>,
    /// The `WM_CLASS` its windows have, when it differs from the ID.
    window_class: Option<String>,
    path: PathBuf,
}

/// Where desktop entries and icons are, most important first.
fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());

    let mut dirs = vec![data_home];
    dirs.extend(system.split(':').map(PathBuf::from));
    // Flatpak and Snap apps, which a session started without their profile
    // scripts does not list.
    dirs.push(home.join(".local/share/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop"));

    let mut seen = HashSet::new();
    dirs.retain(|dir| seen.insert(dir.clone()));
    dirs
}

/// The `[Desktop Entry]` group of a desktop file, as key-value pairs.
fn desktop_entry_group(contents: &str) -> HashMap<&str, &str> {
    let mut in_group = false;
    let mut values = HashMap::new();
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values.entry(key.trim()).or_insert(value.trim());
        }
    }
    values
}

/// The program an `Exec` line runs, without its directory.
fn program_name(exec: &str) -> Option<String> {
    let mut rest = exec.trim();
    loop {
        let (word, after) = match rest.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted.find('"')?;
                (&quoted[..end], &quoted[end + 1..])
            }
            None => {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            }
        };
        if word.is_empty() {
            return None;
        }
        rest = after.trim_start();
        // `env VAR=value program` runs what comes after the assignments.
        let assignment = word.contains('=') && !word.starts_with('/');
        if word == "env" || word.ends_with("/env") || assignment {
            continue;
        }
        return Path::new(word)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
    }
}

fn parse_entry(id: String, path: PathBuf, contents: &str) -> Option<DesktopEntry> {
    let group = desktop_entry_group(contents);
    let shown = |key| group.get(key).is_none_or(|value| *value != "true");
    if group.get("Type") != Some(&"Application") || !shown("NoDisplay") || !shown("Hidden") {
        return None;
    }

    Some(DesktopEntry {
        id,
        name: group.get("Name")?.to_string(),
        icon: group
            .get("Icon")
            .map(|icon| icon.to_string())
            .filter(|icon| !icon.is_empty()),
        program: group.get("Exec").and_then(|exec| program_name(exec)),
        window_class: group
            .get("StartupWMClass")
            .map(|class| class.to_string())
            .filter(|class| !class.is_empty()),
        path,
    })
}

/// Desktop files under `dir`, with the IDs the specification gives them:
/// the path below `applications`, with `/` turned into `-`.
fn collect_desktop_files(dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            collect_desktop_files(&path, &format!("{prefix}{name}-"), files);
        } else if let Some(stem) = name.strip_suffix(".desktop") {
            files.push((format!("{prefix}{stem}"), path));
        }
    }
}

/// Every app with a desktop entry. An entry in a more important directory
/// hides one with the same ID further down, even a hidden one.
fn desktop_entries() -> Vec<DesktopEntry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for dir in data_dirs() {
        let mut files = Vec::new();
        collect_desktop_files(&dir.join("applications"), "", &mut files);
        files.sort();
        for (id, path) in files {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            entries.extend(parse_entry(id, path, &contents));
        }
    }
    entries
}

pub fn list_installed_apps() -> Vec<InstalledApp> {
    let themes = IconThemes::current();
    let mut apps: Vec<InstalledApp> = desktop_entries()
        .into_iter()
        .map(|entry| InstalledApp {
            icon_data_url: entry.icon.as_deref().and_then(|icon| themes.data_url(icon)),
            name: entry.name,
            bundle_id: entry.id,
            path: entry.path.to_string_lossy().into_owned(),
        })
        .collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// One directory of an icon theme, as its `index.theme` describes it.
#[derive(Debug, Clone, PartialEq)]
struct ThemeDir {
    path: PathBuf,
    size: u32,
    scalable: bool,
}

impl ThemeDir {
    /// Lower is better: the size looked for, then larger ones, then a
    /// scalable picture, then smaller ones.
    fn rank(&self) -> (u8, u32) {
        if self.size == ICON_SIZE {
            (0, 0)
        } else if self.size > ICON_SIZE && !self.scalable {
            (1, self.size - ICON_SIZE)
        } else if self.scalable {
            (2, 0)
        } else {
            (3, ICON_SIZE - self.size)
        }
    }
}

/// The application directories an `index.theme` lists, best first.
fn theme_dirs(root: &Path, index: &str) -> Vec<ThemeDir> {
    let mut groups: HashMap<&str, HashMap<&str, &str>> = HashMap::new();
    let mut current = None;
    for line in index.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = Some(name);
            continue;
        }
        if let (Some(group), Some((key, value))) = (current, line.split_once('=')) {
            groups
                .entry(group)
                .or_default()
                .insert(key.trim(), value.trim());
        }
    }

    let listed = groups
        .get("Icon Theme")
        .and_then(|theme| theme.get("Directories"))
        .copied()
        .unwrap_or_default();
    let mut dirs: Vec<ThemeDir> = listed
        .split(',')
        .filter_map(|name| {
            let group = groups.get(name.trim())?;
            if group.get("Context") != Some(&"Applications")
                || group.get("Scale").is_some_and(|scale| *scale != "1")
            {
                return None;
            }
            Some(ThemeDir {
                path: root.join(name.trim()),
                size: group.get("Size")?.parse().ok()?,
                scalable: group.get("Type") == Some(&"Scalable"),
            })
        })
        .collect();
    dirs.sort_by_key(ThemeDir::rank);
    dirs
}

/// The icon themes to look in: the one the desktop uses, the ones it
/// inherits from, and the fallback every app installs into.
struct IconThemes {
    dirs: Vec<ThemeDir>,
}

/// The icon theme the desktop is set to use.
fn configured_theme() -> Option<String> {
    let gsettings = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .trim_matches('\'')
                .to_string()
        });

    let config = std::env::home_dir()?.join(".config");
    let from_file = |file: &str, key: &str| {
        let contents = std::fs::read_to_string(config.join(file)).ok()?;
        contents.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().to_string())
        })
    };

    gsettings
        .or_else(|| from_file("kdeglobals", "Theme"))
        .or_else(|| from_file("gtk-3.0/settings.ini", "gtk-icon-theme-name"))
        .filter(|theme| !theme.is_empty())
}

impl IconThemes {
    fn current() -> Self {
        let home = std::env::home_dir().unwrap_or_default();
        let mut roots = vec![home.join(".icons")];
        roots.extend(data_dirs().into_iter().map(|dir| dir.join("icons")));

        let mut pending: Vec<String> = configured_theme().into_iter().collect();
        pending.push("hicolor".to_string());
        let mut visited = HashSet::new();
        let mut dirs = Vec::new();
        while !pending.is_empty() {
            let theme = pending.remove(0);
            if !visited.insert(theme.clone()) {
                continue;
            }
            let mut inherits = Vec::new();
            for root in &roots {
                let root = root.join(&theme);
                let Ok(index) = std::fs::read_to_string(root.join("index.theme")) else {
                    continue;
                };
                dirs.extend(theme_dirs(&root, &index));
                if let Some(parents) = desktop_entry_value(&index, "Inherits") {
                    inherits.extend(parents.split(',').map(|parent| parent.trim().to_string()));
                }
            }
            // Inherited themes come before the fallback, which stays last.
            let fallback = pending.iter().position(|theme| theme == "hicolor");
            let at = fallback.unwrap_or(pending.len());
            pending.splice(at..at, inherits);
        }
        Self { dirs }
    }

    /// The file of the icon called `icon`, or the file `icon` names.
    fn find(&self, icon: &str) -> Option<PathBuf> {
        let path = Path::new(icon);
        if path.is_absolute() {
            return path.is_file().then(|| path.to_path_buf());
        }
        let files = |dir: &Path| {
            [
                dir.join(format!("{icon}.png")),
                dir.join(format!("{icon}.svg")),
            ]
        };

        self.dirs
            .iter()
            .flat_map(|dir| files(&dir.path))
            .chain(files(Path::new("/usr/share/pixmaps")))
            .find(|file| file.is_file())
    }

    /// The icon as a `data:` URL the interface can show.
    fn data_url(&self, icon: &str) -> Option<String> {
        let file = self.find(icon)?;
        let mime = match file.extension()?.to_str()? {
            "png" => "image/png",
            "svg" => "image/svg+xml",
            _ => return None,
        };
        let bytes = std::fs::read(&file).ok()?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        Some(format!("data:{mime};base64,{encoded}"))
    }
}

/// A value from the first group of an INI-style file.
fn desktop_entry_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim())
    })
}

/// The desktop entries, read again once they are older than a while.
fn cached_entries() -> Vec<DesktopEntry> {
    static CACHE: Mutex<Option<(Instant, Vec<DesktopEntry>)>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match cache.as_ref() {
        Some((read_at, entries)) if read_at.elapsed() < ENTRIES_LIFETIME => entries.clone(),
        _ => {
            let entries = desktop_entries();
            *cache = Some((Instant::now(), entries.clone()));
            entries
        }
    }
}

/// What a window tells about the app that opened it.
#[derive(Debug, Default, Clone, PartialEq)]
struct WindowOwner {
    /// The application ID GTK sets, which is the desktop file ID.
    application_id: Option<String>,
    /// `WM_CLASS`: the instance name, then the class.
    instance: Option<String>,
    class: Option<String>,
    /// The file name of the program the window's process runs.
    program: Option<String>,
}

/// The desktop entry of the app that owns a window.
fn entry_for<'a>(owner: &WindowOwner, entries: &'a [DesktopEntry]) -> Option<&'a DesktopEntry> {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let by = |matches: &dyn Fn(&DesktopEntry) -> bool| entries.iter().find(|entry| matches(entry));
    let names: Vec<&str> = [owner.class.as_deref(), owner.instance.as_deref()]
        .into_iter()
        .flatten()
        .collect();

    owner
        .application_id
        .as_deref()
        .and_then(|id| by(&|entry| entry.id == id))
        .or_else(|| {
            by(&|entry| {
                entry
                    .window_class
                    .as_deref()
                    .is_some_and(|class| names.iter().any(|name| same(class, name)))
            })
        })
        .or_else(|| by(&|entry| names.iter().any(|name| same(&entry.id, name))))
        .or_else(|| {
            // `org.gnome.Nautilus` for a window of class `org.gnome.Nautilus`
            // was handled above; `nautilus` matches its last part.
            by(&|entry| {
                let last = entry.id.rsplit('.').next().unwrap_or(&entry.id);
                names.iter().any(|name| same(last, name))
            })
        })
        .or_else(|| {
            let program = owner.program.as_deref()?;
            by(&|entry| entry.program.as_deref() == Some(program))
        })
}

fn frontmost_from(owner: WindowOwner, entries: &[DesktopEntry]) -> Option<FrontmostApp> {
    if let Some(entry) = entry_for(&owner, entries) {
        return Some(FrontmostApp {
            name: entry.name.clone(),
            bundle_id: entry.id.clone(),
            url: None,
            domain: None,
        });
    }
    // An app without a desktop entry still has a name of its own.
    let id = owner.application_id.or(owner.class).or(owner.program)?;
    Some(FrontmostApp {
        name: id.clone(),
        bundle_id: id,
        url: None,
        domain: None,
    })
}

/// The app whose window has the focus. Only X11 says which window that is.
pub fn frontmost_app() -> Option<FrontmostApp> {
    let owner = x11::active_window_owner()?;
    frontmost_from(owner, &cached_entries())
}

mod x11 {
    use std::sync::OnceLock;

    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    use super::WindowOwner;

    struct Server {
        connection: RustConnection,
        root: xproto::Window,
        active_window: xproto::Atom,
        pid: xproto::Atom,
        application_id: xproto::Atom,
        utf8_string: xproto::Atom,
    }

    fn server() -> Option<&'static Server> {
        static SERVER: OnceLock<Option<Server>> = OnceLock::new();
        SERVER
            .get_or_init(|| {
                let wayland =
                    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|name| !name.is_empty());
                if wayland {
                    return None;
                }
                let (connection, screen) = x11rb::connect(None).ok()?;
                let atom = |name: &str| -> Option<xproto::Atom> {
                    Some(
                        connection
                            .intern_atom(false, name.as_bytes())
                            .ok()?
                            .reply()
                            .ok()?
                            .atom,
                    )
                };
                Some(Server {
                    root: connection.setup().roots[screen].root,
                    active_window: atom("_NET_ACTIVE_WINDOW")?,
                    pid: atom("_NET_WM_PID")?,
                    application_id: atom("_GTK_APPLICATION_ID")?,
                    utf8_string: atom("UTF8_STRING")?,
                    connection,
                })
            })
            .as_ref()
    }

    fn property(
        server: &Server,
        window: xproto::Window,
        property: impl Into<xproto::Atom>,
        kind: impl Into<xproto::Atom>,
    ) -> Option<Vec<u8>> {
        let reply = server
            .connection
            .get_property(false, window, property, kind, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        (!reply.value.is_empty()).then_some(reply.value)
    }

    fn number(
        server: &Server,
        window: xproto::Window,
        name: xproto::Atom,
        kind: AtomEnum,
    ) -> Option<u32> {
        let value = property(server, window, name, kind)?;
        Some(u32::from_ne_bytes(value.get(..4)?.try_into().ok()?))
    }

    fn text(value: &[u8]) -> Option<String> {
        let text = String::from_utf8_lossy(value)
            .trim_matches('\0')
            .to_string();
        (!text.is_empty()).then_some(text)
    }

    pub fn active_window_owner() -> Option<WindowOwner> {
        let server = server()?;
        let window = number(server, server.root, server.active_window, AtomEnum::WINDOW)?;
        if window == 0 {
            return None;
        }

        let class = property(server, window, AtomEnum::WM_CLASS, AtomEnum::STRING);
        let mut class_parts = class
            .as_deref()
            .unwrap_or_default()
            .split(|byte| *byte == 0)
            .filter_map(text);
        let program = number(server, window, server.pid, AtomEnum::CARDINAL).and_then(|pid| {
            let executable = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
            Some(executable.file_name()?.to_string_lossy().into_owned())
        });

        Some(WindowOwner {
            application_id: property(server, window, server.application_id, server.utf8_string)
                .as_deref()
                .and_then(text),
            instance: class_parts.next(),
            class: class_parts.next(),
            program,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, program: Option<&str>, class: Option<&str>) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: name.to_string(),
            icon: None,
            program: program.map(str::to_string),
            window_class: class.map(str::to_string),
            path: PathBuf::from(format!("/usr/share/applications/{id}.desktop")),
        }
    }

    #[test]
    fn reads_the_desktop_entry_group_only() {
        let contents = "[Desktop Entry]\nType=Application\nName=Text Editor\nName[de]=Texteditor\n\
                        Icon=org.gnome.TextEditor\nExec=gnome-text-editor %U\n\
                        StartupWMClass=gnome-text-editor\n\n[Desktop Action new-window]\nName=New Window\n";
        let entry = parse_entry(
            "org.gnome.TextEditor".into(),
            PathBuf::from("/x.desktop"),
            contents,
        )
        .unwrap();

        assert_eq!(entry.name, "Text Editor");
        assert_eq!(entry.icon.as_deref(), Some("org.gnome.TextEditor"));
        assert_eq!(entry.program.as_deref(), Some("gnome-text-editor"));
        assert_eq!(entry.window_class.as_deref(), Some("gnome-text-editor"));
    }

    #[test]
    fn hidden_entries_and_other_types_are_left_out() {
        let parse = |contents: &str| parse_entry("x".into(), PathBuf::new(), contents);

        assert!(parse("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n").is_none());
        assert!(parse("[Desktop Entry]\nType=Application\nName=X\nHidden=true\n").is_none());
        assert!(parse("[Desktop Entry]\nType=Link\nName=X\n").is_none());
        assert!(parse("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=false\n").is_some());
    }

    #[test]
    fn programs_are_named_without_their_directory_or_environment() {
        assert_eq!(
            program_name("/usr/bin/code --new-window %F").as_deref(),
            Some("code")
        );
        assert_eq!(
            program_name("env BAMF_DESKTOP_FILE_HINT=/x.desktop /snap/bin/firefox %u").as_deref(),
            Some("firefox")
        );
        assert_eq!(
            program_name("\"/opt/My App/app\" %U").as_deref(),
            Some("app")
        );
        assert_eq!(program_name(""), None);
    }

    #[test]
    fn windows_are_matched_to_their_apps() {
        let entries = [
            entry(
                "org.gnome.TextEditor",
                "Text Editor",
                Some("gnome-text-editor"),
                None,
            ),
            entry("code", "Visual Studio Code", Some("code"), Some("Code")),
            entry("org.gnome.Nautilus", "Files", Some("nautilus"), None),
            entry("debian-xterm", "XTerm", Some("xterm"), Some("XTerm")),
            entry("firefox_firefox", "Firefox", Some("firefox"), None),
        ];
        let owner = |application_id: Option<&str>, class: Option<&str>, program: Option<&str>| {
            WindowOwner {
                application_id: application_id.map(str::to_string),
                instance: class.map(str::to_lowercase),
                class: class.map(str::to_string),
                program: program.map(str::to_string),
            }
        };
        let id = |owner: WindowOwner| entry_for(&owner, &entries).map(|entry| entry.id.as_str());

        assert_eq!(
            id(owner(
                Some("org.gnome.TextEditor"),
                Some("Gnome-text-editor"),
                None
            )),
            Some("org.gnome.TextEditor")
        );
        assert_eq!(id(owner(None, Some("Code"), Some("code"))), Some("code"));
        assert_eq!(
            id(owner(None, Some("XTerm"), Some("xterm"))),
            Some("debian-xterm")
        );
        assert_eq!(
            id(owner(None, Some("Nautilus"), None)),
            Some("org.gnome.Nautilus")
        );
        assert_eq!(
            id(owner(None, Some("Navigator"), Some("firefox"))),
            Some("firefox_firefox")
        );
        assert_eq!(id(owner(None, Some("Unknown"), Some("unknown"))), None);
    }

    #[test]
    fn an_app_without_a_desktop_entry_is_named_by_its_window() {
        let owner = WindowOwner {
            class: Some("Scratch".into()),
            program: Some("scratch".into()),
            ..WindowOwner::default()
        };
        let app = frontmost_from(owner, &[]).unwrap();

        assert_eq!(
            (app.name.as_str(), app.bundle_id.as_str()),
            ("Scratch", "Scratch")
        );
        assert!(frontmost_from(WindowOwner::default(), &[]).is_none());
    }

    #[test]
    fn icon_directories_prefer_the_list_size() {
        let index = "[Icon Theme]\nName=T\nDirectories=16x16/apps,64x64/apps,256x256/apps,scalable/apps,48x48/devices,64x64@2x/apps\n\n\
                     [16x16/apps]\nSize=16\nContext=Applications\nType=Fixed\n\n\
                     [64x64/apps]\nSize=64\nContext=Applications\nType=Fixed\n\n\
                     [256x256/apps]\nSize=256\nContext=Applications\nType=Fixed\n\n\
                     [scalable/apps]\nSize=128\nContext=Applications\nType=Scalable\n\n\
                     [48x48/devices]\nSize=48\nContext=Devices\nType=Fixed\n\n\
                     [64x64@2x/apps]\nSize=64\nScale=2\nContext=Applications\nType=Fixed\n";
        let dirs: Vec<String> = theme_dirs(Path::new("/t"), index)
            .into_iter()
            .map(|dir| dir.path.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            dirs,
            [
                "/t/64x64/apps",
                "/t/256x256/apps",
                "/t/scalable/apps",
                "/t/16x16/apps"
            ]
        );
    }
}
