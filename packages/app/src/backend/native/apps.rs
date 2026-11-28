#[cfg(not(target_os = "windows"))]
use base64::Engine;
use chrono::Utc;
use rusqlite::params;
#[cfg(not(target_os = "windows"))]
use std::collections::HashSet;
#[cfg(not(target_os = "windows"))]
use std::fs;
#[cfg(not(target_os = "windows"))]
use std::io::BufReader;
use std::panic;
#[cfg(not(target_os = "windows"))]
use std::path::{Path, PathBuf};

use super::db::DbState;
use crate::backend::{
    AppDictionary, AppInstruction, DictionaryWord, FrontmostApp, InstalledApp, MAX_WORD_CHARS,
};

pub struct StyleDef {
    pub key: &'static str,
    pub prompts: &'static [(&'static str, &'static str)],
}

pub const DEFAULT_STYLE_KEY: &str = "default";
pub const DEFAULT_LOCALE: &str = "en";

pub const STYLES: &[StyleDef] = &[
    StyleDef {
        key: "professional",
        prompts: &[
            (
                "en",
                "Pursuant to our prior discussion, I am writing to formally follow up on the third quarter strategic review. Please be advised that the executive committee has requested a comprehensive analysis at your earliest convenience. I would like to express my appreciation for your continued collaboration on this initiative.",
            ),
            (
                "ru",
                "Уважаемый коллега, в продолжение нашего предыдущего обсуждения я хотел бы официально запросить комплексный обзор стратегического плана за третий квартал. Прошу учесть, что исполнительный комитет ожидает детальный анализ в кратчайшие сроки. Благодарю вас за продуктивное сотрудничество в данном вопросе.",
            ),
            (
                "es",
                "En relación con nuestra conversación anterior, le escribo para realizar un seguimiento formal de la revisión estratégica del tercer trimestre. Le informo que el comité ejecutivo ha solicitado un análisis exhaustivo a la mayor brevedad posible. Le agradezco su continua colaboración en esta iniciativa.",
            ),
            (
                "fr",
                "Faisant suite à notre précédente discussion, je vous écris pour faire formellement le point sur l'examen stratégique du troisième trimestre. Je vous informe que le comité exécutif a demandé une analyse complète dans les meilleurs délais. Je vous remercie pour votre collaboration continue sur cette initiative.",
            ),
            (
                "de",
                "Im Anschluss an unsere vorherige Besprechung schreibe ich Ihnen, um offiziell auf die strategische Überprüfung des dritten Quartals zurückzukommen. Bitte beachten Sie, dass der Vorstand zum frühestmöglichen Zeitpunkt eine umfassende Analyse angefordert hat. Ich danke Ihnen für die fortgesetzte Zusammenarbeit an dieser Initiative.",
            ),
            (
                "it",
                "In seguito alla nostra discussione precedente, le scrivo per dare un seguito formale alla revisione strategica del terzo trimestre. La informo che il comitato esecutivo ha richiesto un'analisi approfondita nel più breve tempo possibile. La ringrazio per la continua collaborazione su questa iniziativa.",
            ),
            (
                "zh",
                "续我们之前的讨论，我谨此正式跟进第三季度的战略评估。请知悉，执行委员会已请求尽快进行全面分析。感谢您在此项工作中的持续协作。",
            ),
        ],
    },
    StyleDef {
        key: "casual",
        prompts: &[
            (
                "en",
                "hey just wanted to ping you real quick. i'm gonna grab coffee in like ten you wanna come. honestly i'm so dead didn't sleep at all last night. anyway lemme know whenever. oh and that thing yeah i'm so down just hit me up.",
            ),
            (
                "ru",
                "привет короче хотел тебе быстренько написать. сейчас побегу за кофе минут через десять ты идёшь. честно я просто умираю не спал всю ночь. в общем напиши как сможешь. а ну и про то что мы вчера обсуждали да я полностью за пиши когда захочешь.",
            ),
            (
                "es",
                "oye solo quería escribirte rapidito. voy a por café en como diez minutos te apuntas. en serio estoy muerto no dormí nada anoche. en fin avísame cuando quieras. ah y eso de ayer sí me apunto escríbeme cuando puedas.",
            ),
            (
                "fr",
                "salut juste pour te ping rapidement. je vais prendre un café dans genre dix minutes tu viens. franchement je suis mort j'ai pas dormi du tout. enfin bref dis-moi quand tu veux. ah et ce truc d'hier ouais je suis chaud écris-moi quand tu peux.",
            ),
            (
                "de",
                "hey wollte dir nur kurz schreiben. ich geh in so zehn minuten kaffee holen kommst mit. ehrlich gesagt ich bin total fertig hab überhaupt nicht geschlafen. naja sag bescheid wann du zeit hast. ach und das thema von gestern bin total dabei schreib einfach wenn du willst.",
            ),
            (
                "it",
                "ciao volevo solo scriverti al volo. tra dieci minuti vado a prendere un caffè vieni anche tu. onestamente sono morto non ho dormito niente stanotte. comunque fammi sapere quando vuoi. ah e quella cosa di ieri sì ci sto scrivimi quando puoi.",
            ),
            (
                "zh",
                "嘿就是想快速跟你说一下。我大概十分钟后去喝咖啡你要来吗。说实话我快累死了昨晚根本没睡。反正随时告诉我。哦还有昨天那事我没问题随时找我。",
            ),
        ],
    },
    StyleDef {
        key: "agents",
        prompts: &[
            (
                "en",
                "In src/lib/auth.ts refactor the handleLogin function to use async/await instead of promises. Add a useAuth hook in src/hooks/useAuth.ts that returns the current user and loading state. Update the LoginPage component to redirect to /dashboard when authenticated. Run bun run lint after the changes.",
            ),
            (
                "ru",
                "В файле src/lib/auth.ts перепиши функцию handleLogin на async/await вместо промисов. Добавь хук useAuth в src/hooks/useAuth.ts который возвращает текущего пользователя и состояние загрузки. Обнови компонент LoginPage чтобы он перенаправлял на /dashboard после авторизации. Запусти bun run lint после изменений.",
            ),
            (
                "es",
                "En src/lib/auth.ts refactoriza la función handleLogin para usar async/await en lugar de promesas. Agrega un hook useAuth en src/hooks/useAuth.ts que devuelva el usuario actual y el estado de carga. Actualiza el componente LoginPage para redirigir a /dashboard tras la autenticación. Ejecuta bun run lint después de los cambios.",
            ),
            (
                "fr",
                "Dans src/lib/auth.ts refactorise la fonction handleLogin pour utiliser async/await au lieu des promesses. Ajoute un hook useAuth dans src/hooks/useAuth.ts qui retourne l'utilisateur actuel et l'état de chargement. Mets à jour le composant LoginPage pour rediriger vers /dashboard après authentification. Exécute bun run lint après les modifications.",
            ),
            (
                "de",
                "In src/lib/auth.ts refaktoriere die handleLogin Funktion auf async/await statt Promises. Füge einen useAuth Hook in src/hooks/useAuth.ts hinzu der den aktuellen Benutzer und Ladezustand zurückgibt. Aktualisiere die LoginPage Komponente so dass nach der Authentifizierung auf /dashboard weitergeleitet wird. Führe bun run lint nach den Änderungen aus.",
            ),
            (
                "it",
                "In src/lib/auth.ts rifattorizza la funzione handleLogin per usare async/await invece di promesse. Aggiungi un hook useAuth in src/hooks/useAuth.ts che restituisce l'utente corrente e lo stato di caricamento. Aggiorna il componente LoginPage per reindirizzare a /dashboard dopo l'autenticazione. Esegui bun run lint dopo le modifiche.",
            ),
            (
                "zh",
                "在 src/lib/auth.ts 中将 handleLogin 函数重构为使用 async/await 而不是 promises。在 src/hooks/useAuth.ts 中添加 useAuth 钩子，返回当前用户和加载状态。更新 LoginPage 组件，在认证后重定向到 /dashboard。完成后运行 bun run lint。",
            ),
        ],
    },
    StyleDef {
        key: "default",
        prompts: &[("en", "")],
    },
];

fn pick_prompt_for_locale(prompts: &[(&'static str, &'static str)], locale: &str) -> &'static str {
    prompts
        .iter()
        .find(|(l, _)| *l == locale)
        .or_else(|| prompts.iter().find(|(l, _)| *l == DEFAULT_LOCALE))
        .map(|(_, p)| *p)
        .unwrap_or("")
}

pub fn style_prompt_text(key: &str, locale: &str) -> &'static str {
    let style = STYLES
        .iter()
        .find(|s| s.key == key)
        .or_else(|| STYLES.iter().find(|s| s.key == DEFAULT_STYLE_KEY))
        .expect("default style missing");
    pick_prompt_for_locale(style.prompts, locale)
}

pub fn resolve_style_key(
    conn: &rusqlite::Connection,
    bundle_id: Option<&str>,
    domain: Option<&str>,
    fallback_style: Option<&str>,
) -> String {
    if let Some(d) = domain.filter(|s| !s.is_empty()) {
        for group in crate::backend::domain_lookup_groups(d) {
            for key in group {
                if let Ok(style) = conn.query_row(
                    "SELECT style FROM site_styles WHERE domain = ?1",
                    params![key],
                    |row| row.get::<_, String>(0),
                ) {
                    return style;
                }
            }
        }
    }
    if let Some(bid) = bundle_id.filter(|s| !s.is_empty())
        && let Ok(style) = conn.query_row(
            "SELECT style FROM app_instructions WHERE bundle_id = ?1",
            params![bid],
            |row| row.get::<_, String>(0),
        )
    {
        return style;
    }
    fallback_style
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_STYLE_KEY)
        .to_string()
}

pub fn run_migrations(conn: &rusqlite::Connection) -> Result<(), String> {
    let has_app_name = conn
        .prepare("PRAGMA table_info(app_dictionary_words)")
        .and_then(|mut stmt| {
            let cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(cols)
        })
        .map(|cols| cols.iter().any(|c| c == "app_name"))
        .unwrap_or(true);
    if !has_app_name {
        conn.execute_batch("DROP TABLE IF EXISTS app_dictionary_words;")
            .map_err(|e| format!("Failed to drop stale app_dictionary_words: {}", e))?;
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_instructions (
            id TEXT PRIMARY KEY,
            bundle_id TEXT NOT NULL UNIQUE,
            app_name TEXT NOT NULL,
            style TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_app_instructions_bundle_id ON app_instructions(bundle_id);
        CREATE TABLE IF NOT EXISTS app_dictionary_words (
            id TEXT PRIMARY KEY,
            bundle_id TEXT NOT NULL,
            app_name TEXT NOT NULL,
            word TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_app_dictionary_words_bundle_id ON app_dictionary_words(bundle_id);",
    )
    .map_err(|e| format!("Failed to create app tables: {}", e))?;
    Ok(())
}

pub fn fetch_app_dictionary_words(conn: &rusqlite::Connection, bundle_id: &str) -> Option<String> {
    let mut stmt = conn
        .prepare(
            "SELECT word FROM app_dictionary_words WHERE bundle_id = ?1 \
             ORDER BY created_at DESC LIMIT 50",
        )
        .ok()?;
    let words: Vec<String> = stmt
        .query_map(params![bundle_id], |row| row.get::<_, String>(0))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(", "))
    }
}

#[cfg(not(target_os = "windows"))]
fn read_bundle_id(plist: &plist::Dictionary) -> Option<String> {
    plist
        .get("CFBundleIdentifier")?
        .as_string()
        .map(|s| s.to_string())
}

#[cfg(not(target_os = "windows"))]
fn read_display_name(plist: &plist::Dictionary, fallback: &str) -> String {
    if let Some(name) = plist.get("CFBundleDisplayName").and_then(|v| v.as_string()) {
        return name.to_string();
    }
    if let Some(name) = plist.get("CFBundleName").and_then(|v| v.as_string()) {
        return name.to_string();
    }
    fallback.trim_end_matches(".app").to_string()
}

#[cfg(not(target_os = "windows"))]
fn read_info_plist(app_path: &Path) -> Option<plist::Dictionary> {
    let plist_path = app_path.join("Contents").join("Info.plist");
    let result = panic::catch_unwind(|| plist::Value::from_file(&plist_path).ok()).ok()??;
    result.into_dictionary()
}

#[cfg(not(target_os = "windows"))]
fn icns_path_from_plist(app_path: &Path, plist: &plist::Dictionary) -> Option<PathBuf> {
    let icon_file = plist.get("CFBundleIconFile")?.as_string()?;
    let file_name = if icon_file.ends_with(".icns") {
        icon_file.to_string()
    } else {
        format!("{}.icns", icon_file)
    };
    let path = app_path.join("Contents").join("Resources").join(&file_name);
    if path.exists() { Some(path) } else { None }
}

#[cfg(not(target_os = "windows"))]
fn pick_icon_type(family: &icns::IconFamily) -> Option<icns::IconType> {
    let available = family.available_icons();
    available
        .iter()
        .filter(|t| !t.is_mask() && t.pixel_width() >= 32 && t.pixel_width() <= 128)
        .min_by_key(|t| t.pixel_width())
        .or_else(|| {
            available
                .iter()
                .filter(|t| !t.is_mask())
                .max_by_key(|t| t.pixel_width())
        })
        .copied()
}

#[cfg(not(target_os = "windows"))]
fn extract_icon_data_url(app_path: &Path, plist: &plist::Dictionary) -> Option<String> {
    let icns_path = icns_path_from_plist(app_path, plist)?;
    let result = panic::catch_unwind(|| -> Option<Vec<u8>> {
        let file = std::fs::File::open(&icns_path).ok()?;
        let family = icns::IconFamily::read(BufReader::new(file)).ok()?;
        let icon_type = pick_icon_type(&family)?;
        let image = family.get_icon_with_type(icon_type).ok()?;
        let mut png_buf = Vec::new();
        image.write_png(&mut png_buf).ok()?;
        Some(png_buf)
    })
    .ok()??;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&result);
    Some(format!("data:image/png;base64,{}", b64))
}

#[cfg(not(target_os = "windows"))]
fn scan_apps_dir(dir: &Path, results: &mut Vec<InstalledApp>, seen: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".app") {
            continue;
        }
        let Some(plist) = read_info_plist(&path) else {
            continue;
        };
        let Some(bundle_id) = read_bundle_id(&plist) else {
            continue;
        };
        if !seen.insert(bundle_id.clone()) {
            continue;
        }
        let name = read_display_name(&plist, file_name);
        let icon_data_url = extract_icon_data_url(&path, &plist);
        let path_str = path.to_string_lossy().to_string();
        results.push(InstalledApp {
            name,
            bundle_id,
            path: path_str,
            icon_data_url,
        });
    }
}

#[cfg(not(target_os = "windows"))]
pub fn list_installed_apps() -> Vec<InstalledApp> {
    let mut results: Vec<InstalledApp> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut dirs: Vec<PathBuf> = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::home_dir() {
        dirs.push(home.join("Applications"));
    }
    dirs.push(PathBuf::from("/System/Applications"));

    for dir in dirs {
        scan_apps_dir(&dir, &mut results, &mut seen);
    }

    results.sort_by_key(|a| a.name.to_lowercase());
    results
}

#[cfg(target_os = "macos")]
pub fn get_frontmost_app() -> Option<FrontmostApp> {
    panic::catch_unwind(|| {
        use objc2_app_kit::NSWorkspace;

        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let bundle_id = app
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if bundle_id.is_empty() {
            return None;
        }
        let pid: i32 = app.processIdentifier() as i32;
        let (url, domain) = if super::browser::is_known_browser(&bundle_id) {
            let url = super::browser::get_frontmost_browser_url(&bundle_id, pid);
            let domain = url.as_deref().and_then(crate::backend::browser_domain);
            (url, domain)
        } else {
            (None, None)
        };
        Some(FrontmostApp {
            name,
            bundle_id,
            url,
            domain,
        })
    })
    .unwrap_or(None)
}

#[cfg(target_os = "windows")]
mod win_apps {
    use base64::Engine;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
        GetDIBits, GetObjectW, ReleaseDC,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ,
        REG_VALUE_TYPE, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Shell::{
        IShellLinkW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW, SHLoadIndirectString,
        SLGP_RAWPATH, ShellLink,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, EnumChildWindows, GetForegroundWindow, GetIconInfo, GetWindowThreadProcessId,
        HICON, ICONINFO,
    };
    use windows::core::{BOOL, Interface, PCWSTR, PWSTR};

    use crate::backend::{FrontmostApp, InstalledApp};

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn friendly_name_for_exe(exe_lower: &str, stem: &str) -> String {
        match exe_lower {
            "notepad.exe" => "Notepad".to_string(),
            "notepad++.exe" => "Notepad++".to_string(),
            "msedge.exe" => "Microsoft Edge".to_string(),
            "chrome.exe" => "Google Chrome".to_string(),
            "brave.exe" => "Brave".to_string(),
            "firefox.exe" => "Firefox".to_string(),
            "vivaldi.exe" => "Vivaldi".to_string(),
            "opera.exe" => "Opera".to_string(),
            "arc.exe" => "Arc".to_string(),
            "zen.exe" => "Zen Browser".to_string(),
            "code.exe" => "Visual Studio Code".to_string(),
            "cursor.exe" => "Cursor".to_string(),
            "powershell.exe" => "Windows PowerShell".to_string(),
            "pwsh.exe" => "PowerShell".to_string(),
            "cmd.exe" => "Command Prompt".to_string(),
            "wt.exe" | "windowsterminal.exe" => "Windows Terminal".to_string(),
            "explorer.exe" => "File Explorer".to_string(),
            "mspaint.exe" => "Paint".to_string(),
            "wordpad.exe" => "WordPad".to_string(),
            "soffice.bin" | "swriter.exe" => "LibreOffice Writer".to_string(),
            "winword.exe" => "Microsoft Word".to_string(),
            "excel.exe" => "Microsoft Excel".to_string(),
            "outlook.exe" | "olk.exe" => "Outlook".to_string(),
            "slack.exe" => "Slack".to_string(),
            "discord.exe" => "Discord".to_string(),
            "telegram.exe" => "Telegram".to_string(),
            "notion.exe" => "Notion".to_string(),
            "obsidian.exe" => "Obsidian".to_string(),
            "voxfusion-app.exe" => "VoxFusion".to_string(),
            _ => {
                let mut chars = stem.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => exe_lower.to_string(),
                }
            }
        }
    }

    fn should_skip_exe(exe_lower: &str) -> bool {
        if !exe_lower.ends_with(".exe") {
            return true;
        }
        let stem = exe_lower.trim_end_matches(".exe");
        stem.starts_with("unins")
            || stem.contains("uninstall")
            || stem.starts_with("setup")
            || stem.ends_with("_setup")
            || stem.ends_with("-setup")
            || stem.contains("updater")
            || stem == "update"
            || stem.contains("crashpad")
            || stem.contains("helper")
            || stem == "msiexec"
            || stem == "rundll32"
            || stem == "regsvr32"
            || stem == "conhost"
    }

    fn process_exe_path(pid: u32) -> Option<PathBuf> {
        if pid == 0 {
            return None;
        }
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        }
        .is_ok();
        unsafe {
            let _ = CloseHandle(handle);
        }
        if !ok || len == 0 {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }

    struct UwpChildSearch {
        host_pid: u32,
        found_pid: u32,
    }

    unsafe extern "system" fn find_uwp_child_pid(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam.0 as *mut UwpChildSearch) };
        let mut child_pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut child_pid));
        }
        if child_pid != 0 && child_pid != state.host_pid {
            state.found_pid = child_pid;
            return BOOL(0);
        }
        BOOL(1)
    }

    pub fn get_frontmost_app() -> Option<FrontmostApp> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        let mut exe_path = process_exe_path(pid)?;
        let mut exe_name = exe_path.file_name()?.to_str()?.to_string();
        let mut bundle_id = exe_name.to_ascii_lowercase();

        // Packaged UWP apps host their UI inside ApplicationFrameHost.exe;
        // find the real child process so per-app rules match the app itself.
        if bundle_id == "applicationframehost.exe" {
            let mut search = UwpChildSearch {
                host_pid: pid,
                found_pid: 0,
            };
            unsafe {
                let _ = EnumChildWindows(
                    Some(hwnd),
                    Some(find_uwp_child_pid),
                    LPARAM(&mut search as *mut UwpChildSearch as isize),
                );
            }
            if search.found_pid != 0
                && let Some(child_path) = process_exe_path(search.found_pid)
                && let Some(child_name) = child_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            {
                exe_path = child_path;
                exe_name = child_name;
                bundle_id = exe_name.to_ascii_lowercase();
            }
        }

        let stem = exe_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&exe_name);
        let name = friendly_name_for_exe(&bundle_id, stem);

        let (url, domain) = if super::super::browser::is_known_browser(&bundle_id) {
            let url = super::super::browser::get_frontmost_browser_url(&bundle_id, hwnd.0 as isize);
            let domain = url.as_deref().and_then(crate::backend::browser_domain);
            (url, domain)
        } else {
            (None, None)
        };

        Some(FrontmostApp {
            name,
            bundle_id,
            url,
            domain,
        })
    }

    fn resolve_shortcut_target(lnk_path: &Path) -> Option<PathBuf> {
        let shell_link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
        let persist_file: IPersistFile = shell_link.cast().ok()?;
        let wide_lnk: Vec<u16> = lnk_path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe { persist_file.Load(PCWSTR::from_raw(wide_lnk.as_ptr()), STGM_READ) }.ok()?;

        let mut target_buf = [0u16; MAX_PATH as usize];
        unsafe {
            shell_link.GetPath(
                &mut target_buf,
                std::ptr::null_mut(),
                SLGP_RAWPATH.0 as u32,
            )
        }
        .ok()?;
        let end = target_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(target_buf.len());
        if end == 0 {
            return None;
        }
        let raw = String::from_utf16_lossy(&target_buf[..end]);
        let expanded = expand_env_vars(&raw);
        let path = PathBuf::from(expanded);
        path.is_file().then_some(path)
    }

    fn expand_env_vars(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            if let Some(end) = after.find('%') {
                let var = &after[..end];
                if !var.is_empty()
                    && let Ok(val) = std::env::var(var)
                {
                    out.push_str(&val);
                } else {
                    out.push('%');
                    out.push_str(var);
                    out.push('%');
                }
                rest = &after[end + 1..];
            } else {
                out.push('%');
                rest = after;
                break;
            }
        }
        out.push_str(rest);
        out
    }

    fn extract_exe_icon_data_url(exe_path: &Path) -> Option<String> {
        let wide: Vec<u16> = exe_path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut shfi = windows::Win32::UI::Shell::SHFILEINFOW::default();
        let res = unsafe {
            SHGetFileInfoW(
                PCWSTR::from_raw(wide.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut shfi),
                std::mem::size_of::<windows::Win32::UI::Shell::SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            )
        };
        if res == 0 || shfi.hIcon.is_invalid() {
            return None;
        }
        let png_bytes = hicon_to_png(shfi.hIcon);
        unsafe {
            let _ = DestroyIcon(shfi.hIcon);
        }
        let png_bytes = png_bytes?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        Some(format!("data:image/png;base64,{b64}"))
    }

    fn hicon_to_png(hicon: HICON) -> Option<Vec<u8>> {
        let mut info = ICONINFO::default();
        unsafe { GetIconInfo(hicon, &mut info) }.ok()?;

        struct IconBitmaps(ICONINFO);
        impl Drop for IconBitmaps {
            fn drop(&mut self) {
                unsafe {
                    if !self.0.hbmColor.is_invalid() {
                        let _ = DeleteObject(self.0.hbmColor.into());
                    }
                    if !self.0.hbmMask.is_invalid() {
                        let _ = DeleteObject(self.0.hbmMask.into());
                    }
                }
            }
        }
        let bitmaps = IconBitmaps(info);

        if bitmaps.0.hbmColor.is_invalid() {
            return None;
        }

        let mut bmp = BITMAP::default();
        let got = unsafe {
            GetObjectW(
                bitmaps.0.hbmColor.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bmp as *mut BITMAP as *mut std::ffi::c_void),
            )
        };
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            return None;
        }
        let width = bmp.bmWidth as u32;
        let height = bmp.bmHeight as u32;
        let pixel_count = (width as usize).checked_mul(height as usize)?;
        let mut bgra = vec![0u8; pixel_count.checked_mul(4)?];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let hdc = unsafe { GetDC(None) };
        if hdc.is_invalid() {
            return None;
        }
        let scanlines = unsafe {
            GetDIBits(
                hdc,
                bitmaps.0.hbmColor,
                0,
                height,
                Some(bgra.as_mut_ptr().cast()),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        };

        let has_alpha = bgra.chunks_exact(4).any(|px| px[3] != 0);
        let mut mask_bgra: Option<Vec<u8>> = None;
        if !has_alpha && !bitmaps.0.hbmMask.is_invalid() {
            let mut mask_buf = vec![0u8; pixel_count * 4];
            let mask_lines = unsafe {
                GetDIBits(
                    hdc,
                    bitmaps.0.hbmMask,
                    0,
                    height,
                    Some(mask_buf.as_mut_ptr().cast()),
                    &mut bmi,
                    DIB_RGB_COLORS,
                )
            };
            if mask_lines > 0 {
                mask_bgra = Some(mask_buf);
            }
        }
        unsafe {
            let _ = ReleaseDC(None, hdc);
        }

        if scanlines == 0 {
            return None;
        }

        let mut rgba = vec![0u8; bgra.len()];
        for (i, px) in bgra.chunks_exact(4).enumerate() {
            let dst = &mut rgba[i * 4..i * 4 + 4];
            dst[0] = px[2];
            dst[1] = px[1];
            dst[2] = px[0];
            dst[3] = if has_alpha {
                px[3]
            } else if let Some(ref mask) = mask_bgra {
                if mask[i * 4] == 0 { 255 } else { 0 }
            } else {
                255
            };
        }

        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&rgba).ok()?;
        }
        Some(png_bytes)
    }

    /// Normalizes Windows app execution aliases to the actual foreground
    /// process executable name so picker rules match running windows.
    fn canonical_exe_id(exe_lower: &str) -> &str {
        match exe_lower {
            "wt.exe" => "windowsterminal.exe",
            other => other,
        }
    }

    fn push_app(
        exe_path: &Path,
        preferred_name: Option<String>,
        results: &mut Vec<InstalledApp>,
        seen: &mut HashSet<String>,
    ) {
        let Some(file_name) = exe_path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        let exe_lower = file_name.to_ascii_lowercase();
        let bundle_id = canonical_exe_id(&exe_lower).to_string();
        if should_skip_exe(&bundle_id) || !seen.insert(bundle_id.clone()) {
            return;
        }
        let stem = exe_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file_name);
        let name = preferred_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| friendly_name_for_exe(&bundle_id, stem));
        let icon_data_url = extract_exe_icon_data_url(exe_path);
        results.push(InstalledApp {
            name,
            bundle_id,
            path: exe_path.to_string_lossy().to_string(),
            icon_data_url,
        });
    }

    fn scan_start_menu_dir(
        dir: &Path,
        depth: usize,
        results: &mut Vec<InstalledApp>,
        seen: &mut HashSet<String>,
    ) {
        if depth > 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_start_menu_dir(&path, depth + 1, results, seen);
                continue;
            }
            let is_lnk = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
            if !is_lnk {
                continue;
            }
            let shortcut_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.trim().to_string());
            if shortcut_name
                .as_deref()
                .is_some_and(|n| n.to_ascii_lowercase().contains("uninstall"))
            {
                continue;
            }
            if let Some(target) = resolve_shortcut_target(&path) {
                push_app(&target, shortcut_name, results, seen);
            }
        }
    }

    fn scan_app_paths_registry(
        root: HKEY,
        results: &mut Vec<InstalledApp>,
        seen: &mut HashSet<String>,
    ) {
        let subkey: Vec<u16> = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut hkey = HKEY::default();
        if unsafe {
            RegOpenKeyExW(
                root,
                PCWSTR::from_raw(subkey.as_ptr()),
                Some(0),
                KEY_READ,
                &mut hkey,
            )
        }
        .is_err()
        {
            return;
        }

        let mut index = 0u32;
        loop {
            let mut name_buf = [0u16; 260];
            let mut name_len = name_buf.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    hkey,
                    index,
                    Some(PWSTR(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            index += 1;
            if status.is_err() {
                break;
            }
            let app_key_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            if !app_key_name.to_ascii_lowercase().ends_with(".exe") {
                continue;
            }
            let app_key_wide: Vec<u16> = app_key_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut app_hkey = HKEY::default();
            if unsafe {
                RegOpenKeyExW(
                    hkey,
                    PCWSTR::from_raw(app_key_wide.as_ptr()),
                    Some(0),
                    KEY_READ,
                    &mut app_hkey,
                )
            }
            .is_err()
            {
                continue;
            }

            let mut val_type = REG_VALUE_TYPE::default();
            let mut size = 0u32;
            let ok = unsafe {
                RegQueryValueExW(
                    app_hkey,
                    PCWSTR::null(),
                    None,
                    Some(&mut val_type),
                    None,
                    Some(&mut size),
                )
            }
            .is_ok();
            if ok && (val_type == REG_SZ || val_type == REG_EXPAND_SZ) && size > 0 {
                let mut buf = vec![0u8; size as usize];
                if unsafe {
                    RegQueryValueExW(
                        app_hkey,
                        PCWSTR::null(),
                        None,
                        Some(&mut val_type),
                        Some(buf.as_mut_ptr()),
                        Some(&mut size),
                    )
                }
                .is_ok()
                {
                    let units: Vec<u16> = buf[..size as usize]
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect();
                    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
                    let raw = String::from_utf16_lossy(&units[..end]);
                    let trimmed = raw.trim().trim_matches('"');
                    let expanded = expand_env_vars(trimmed);
                    let path = PathBuf::from(expanded);
                    if path.is_file() {
                        push_app(&path, None, results, seen);
                    }
                }
            }
            unsafe {
                let _ = RegCloseKey(app_hkey);
            }
        }

        unsafe {
            let _ = RegCloseKey(hkey);
        }
    }

    fn read_reg_string(hkey: HKEY, value_name: PCWSTR) -> Option<String> {
        let mut val_type = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        unsafe {
            RegQueryValueExW(
                hkey,
                value_name,
                None,
                Some(&mut val_type),
                None,
                Some(&mut size),
            )
            .ok()
            .ok()?;
        }
        if (val_type != REG_SZ && val_type != REG_EXPAND_SZ) || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        unsafe {
            RegQueryValueExW(
                hkey,
                value_name,
                None,
                Some(&mut val_type),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            )
            .ok()
            .ok()?;
        }
        let units: Vec<u16> = buf[..size as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    fn resolve_indirect_string(source: &str) -> Option<String> {
        let wide: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
        let mut out = [0u16; 256];
        unsafe { SHLoadIndirectString(PCWSTR(wide.as_ptr()), &mut out, None) }.ok()?;
        let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
        let resolved = String::from_utf16_lossy(&out[..end]).trim().to_string();
        (!resolved.is_empty()).then_some(resolved)
    }

    fn resolve_packaged_display_name(
        pkg_full_name: &str,
        raw_name: Option<&str>,
    ) -> Option<String> {
        let raw = raw_name?.trim();
        if raw.is_empty() {
            return None;
        }
        if raw.starts_with("@{") {
            return resolve_indirect_string(raw);
        }
        if let Some(rest) = raw
            .strip_prefix("ms-resource:")
            .or_else(|| raw.strip_prefix("MS-RESOURCE:"))
        {
            let pkg_short = pkg_full_name.split('_').next().unwrap_or(pkg_full_name);
            let trimmed = rest.trim_start_matches('/');
            let candidates = [
                format!("@{{{pkg_full_name}?ms-resource://{pkg_short}/resources/{trimmed}}}"),
                format!("@{{{pkg_full_name}?ms-resource://{pkg_short}/{trimmed}}}"),
            ];
            for candidate in candidates {
                if let Some(resolved) = resolve_indirect_string(&candidate) {
                    return Some(resolved);
                }
            }
            return None;
        }
        Some(raw.to_string())
    }

    fn extract_xml_attr<'a>(fragment: &'a str, attr: &str) -> Option<&'a str> {
        let lower = fragment.to_ascii_lowercase();
        let needle = format!("{}=", attr.to_ascii_lowercase());
        let mut offset = 0usize;
        while let Some(pos) = lower[offset..].find(&needle) {
            let abs = offset + pos;
            let boundary_ok = abs == 0
                || fragment.as_bytes()[abs - 1].is_ascii_whitespace()
                || fragment.as_bytes()[abs - 1] == b':';
            if boundary_ok {
                let after = &fragment[abs + needle.len()..];
                let after = after.trim_start();
                let quote = *after.as_bytes().first()?;
                if quote == b'"' || quote == b'\'' {
                    let rest = &after[1..];
                    let end = rest.find(quote as char)?;
                    return Some(&rest[..end]);
                }
            }
            offset = abs + needle.len();
        }
        None
    }

    /// Discovers MSIX / Microsoft Store packaged apps registered for the
    /// current user so they appear in per-app dictionary and style pickers
    /// even when they have no `.lnk` shortcut or App Paths registry entry.
    fn scan_packaged_apps(results: &mut Vec<InstalledApp>, seen: &mut HashSet<String>) {
        let subkey: Vec<u16> =
            r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
        let mut hkey = HKEY::default();
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(subkey.as_ptr()),
                Some(0),
                KEY_READ,
                &mut hkey,
            )
        }
        .is_err()
        {
            return;
        }

        let root_val_name: Vec<u16> = "PackageRootFolder"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let display_val_name: Vec<u16> = "DisplayName"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut index = 0u32;
        loop {
            let mut name_buf = [0u16; 260];
            let mut name_len = name_buf.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    hkey,
                    index,
                    Some(PWSTR(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            index += 1;
            if status.is_err() {
                break;
            }

            let pkg_full_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let pkg_key_wide: Vec<u16> = pkg_full_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut pkg_hkey = HKEY::default();
            if unsafe {
                RegOpenKeyExW(
                    hkey,
                    PCWSTR::from_raw(pkg_key_wide.as_ptr()),
                    Some(0),
                    KEY_READ,
                    &mut pkg_hkey,
                )
            }
            .is_err()
            {
                continue;
            }

            let root_str = read_reg_string(pkg_hkey, PCWSTR::from_raw(root_val_name.as_ptr()));
            let pkg_display =
                read_reg_string(pkg_hkey, PCWSTR::from_raw(display_val_name.as_ptr()));
            unsafe {
                let _ = RegCloseKey(pkg_hkey);
            }

            let Some(root_str) = root_str else {
                continue;
            };
            let root_lower = root_str.to_ascii_lowercase();
            if root_lower.contains(r"\windows\systemapps\")
                || root_lower.contains(r"\windows\immersivecontrolpanel")
            {
                continue;
            }

            let root_path = PathBuf::from(&root_str);
            let manifest_path = root_path.join("AppxManifest.xml");
            let Ok(meta) = std::fs::metadata(&manifest_path) else {
                continue;
            };
            if meta.len() > 512 * 1024 {
                continue;
            }
            let Ok(manifest) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };

            for chunk in manifest.split("<Application").skip(1) {
                if !chunk
                    .as_bytes()
                    .first()
                    .is_some_and(|b| b.is_ascii_whitespace())
                {
                    continue;
                }
                let block = chunk.split("</Application>").next().unwrap_or(chunk);
                if extract_xml_attr(block, "AppListEntry")
                    .is_some_and(|v| v.eq_ignore_ascii_case("none"))
                {
                    continue;
                }
                let Some(exec) = extract_xml_attr(block, "Executable") else {
                    continue;
                };
                if !exec.to_ascii_lowercase().ends_with(".exe")
                    || exec.contains("$targetnametoken$")
                {
                    continue;
                }
                let exe_path = root_path.join(exec);
                if !exe_path.is_file() {
                    continue;
                }
                let app_display = extract_xml_attr(block, "DisplayName")
                    .and_then(|raw| resolve_packaged_display_name(&pkg_full_name, Some(raw)))
                    .or_else(|| {
                        resolve_packaged_display_name(&pkg_full_name, pkg_display.as_deref())
                    });
                push_app(&exe_path, app_display, results, seen);
            }
        }

        unsafe {
            let _ = RegCloseKey(hkey);
        }
    }

    pub fn list_installed_apps() -> Vec<InstalledApp> {
        let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let _com = ComGuard(com_ok);

        let mut results: Vec<InstalledApp> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // 1. Start Menu shortcuts (user + all users).
        if let Ok(program_data) = std::env::var("ProgramData") {
            let dir = PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            scan_start_menu_dir(&dir, 0, &mut results, &mut seen);
        }
        if let Ok(app_data) = std::env::var("APPDATA") {
            let dir = PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            scan_start_menu_dir(&dir, 0, &mut results, &mut seen);
        }

        // 2. Registry App Paths.
        scan_app_paths_registry(HKEY_CURRENT_USER, &mut results, &mut seen);
        scan_app_paths_registry(HKEY_LOCAL_MACHINE, &mut results, &mut seen);

        // 3. Packaged MSIX / Microsoft Store apps.
        scan_packaged_apps(&mut results, &mut seen);

        // 4. Built-in Windows apps that may not have Start Menu `.lnk` targets.
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        let builtins = [
            (
                PathBuf::from(&system_root).join("System32").join("notepad.exe"),
                "Notepad",
            ),
            (
                PathBuf::from(&system_root)
                    .join("System32")
                    .join("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe"),
                "Windows PowerShell",
            ),
            (
                PathBuf::from(&system_root).join("System32").join("cmd.exe"),
                "Command Prompt",
            ),
            (
                PathBuf::from(&system_root).join("explorer.exe"),
                "File Explorer",
            ),
            (
                PathBuf::from(&system_root).join("System32").join("mspaint.exe"),
                "Paint",
            ),
        ];
        for (path, label) in builtins {
            if path.is_file() {
                push_app(&path, Some(label.to_string()), &mut results, &mut seen);
            }
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let wt = PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("WindowsApps")
                .join("wt.exe");
            if wt.exists() {
                push_app(&wt, Some("Windows Terminal".to_string()), &mut results, &mut seen);
            }
        }

        results.sort_by_key(|a| a.name.to_lowercase());
        results
    }
}

#[cfg(target_os = "windows")]
pub fn list_installed_apps() -> Vec<InstalledApp> {
    panic::catch_unwind(win_apps::list_installed_apps).unwrap_or_default()
}

#[cfg(target_os = "windows")]
pub fn get_frontmost_app() -> Option<FrontmostApp> {
    panic::catch_unwind(win_apps::get_frontmost_app).unwrap_or(None)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn get_frontmost_app() -> Option<FrontmostApp> {
    None
}

pub fn list_app_instructions(state: &DbState) -> Result<Vec<AppInstruction>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, bundle_id, app_name, style, created_at, updated_at \
             FROM app_instructions ORDER BY app_name COLLATE NOCASE ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AppInstruction {
                id: row.get(0)?,
                bundle_id: row.get(1)?,
                app_name: row.get(2)?,
                style: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn normalize_style(style: &str) -> Result<&'static str, String> {
    STYLES
        .iter()
        .find(|s| s.key == style)
        .map(|s| s.key)
        .ok_or_else(|| format!("Invalid style: {}", style))
}

pub fn set_app_instruction(
    state: &DbState,
    bundle_id: &str,
    app_name: &str,
    style: &str,
) -> Result<AppInstruction, String> {
    let bundle_id = bundle_id.trim().to_string();
    let app_name = app_name.trim().to_string();
    if bundle_id.is_empty() || app_name.is_empty() {
        return Err("bundle_id and app_name are required".to_string());
    }
    let style = normalize_style(style)?;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();

    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT id, created_at FROM app_instructions WHERE bundle_id = ?1",
            params![bundle_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let (id, created_at) = match existing {
        Some((id, created_at)) => {
            conn.execute(
                "UPDATE app_instructions SET app_name = ?1, style = ?2, updated_at = ?3 WHERE id = ?4",
                params![app_name, style, now, id],
            )
            .map_err(|e| e.to_string())?;
            (id, created_at)
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO app_instructions (id, bundle_id, app_name, style, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, bundle_id, app_name, style, now, now],
            )
            .map_err(|e| e.to_string())?;
            (id, now.clone())
        }
    };

    Ok(AppInstruction {
        id,
        bundle_id,
        app_name,
        style: style.to_string(),
        created_at,
        updated_at: now,
    })
}

pub fn delete_app_instruction(state: &DbState, id: &str) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM app_instructions WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_app_dictionaries(state: &DbState) -> Result<Vec<AppDictionary>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, bundle_id, app_name, word, created_at, updated_at \
             FROM app_dictionary_words \
             ORDER BY app_name COLLATE NOCASE ASC, created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut grouped: Vec<AppDictionary> = Vec::new();
    for (id, bundle_id, app_name, word, created_at, updated_at) in rows {
        let entry = DictionaryWord {
            id,
            word,
            created_at,
            updated_at,
        };
        match grouped.iter_mut().find(|g| g.bundle_id == bundle_id) {
            Some(group) => group.words.push(entry),
            None => grouped.push(AppDictionary {
                bundle_id,
                app_name,
                words: vec![entry],
            }),
        }
    }
    Ok(grouped)
}

pub fn add_app_dictionary_word(
    state: &DbState,
    bundle_id: &str,
    app_name: &str,
    word: &str,
) -> Result<DictionaryWord, String> {
    let bundle_id = bundle_id.trim().to_string();
    let app_name = app_name.trim().to_string();
    let word = word.trim();
    if bundle_id.is_empty() || app_name.is_empty() {
        return Err("bundle_id and app_name are required".to_string());
    }
    if word.is_empty() || word.chars().count() > MAX_WORD_CHARS {
        return Err(format!(
            "Word must be between 1 and {MAX_WORD_CHARS} characters"
        ));
    }

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO app_dictionary_words (id, bundle_id, app_name, word, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, bundle_id, app_name, word, now, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(DictionaryWord {
        id,
        word: word.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update_app_dictionary_word(
    state: &DbState,
    id: &str,
    word: &str,
) -> Result<DictionaryWord, String> {
    let word = word.trim();
    if word.is_empty() || word.chars().count() > MAX_WORD_CHARS {
        return Err(format!(
            "Word must be between 1 and {MAX_WORD_CHARS} characters"
        ));
    }
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let rows_affected = conn
        .execute(
            "UPDATE app_dictionary_words SET word = ?1, updated_at = ?2 WHERE id = ?3",
            params![word, now, id],
        )
        .map_err(|e| e.to_string())?;
    if rows_affected == 0 {
        return Err("Word not found".to_string());
    }
    Ok(DictionaryWord {
        id: id.to_string(),
        word: word.to_string(),
        created_at: String::new(),
        updated_at: now,
    })
}

pub fn delete_app_dictionary_word(state: &DbState, id: &str) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM app_dictionary_words WHERE id = ?1",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_app_dictionary(state: &DbState, bundle_id: &str) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM app_dictionary_words WHERE bundle_id = ?1",
        params![bundle_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_has_one_style_that_later_calls_replace() {
        let state = DbState::in_memory();

        let first = set_app_instruction(&state, " com.apple.mail ", " Mail ", "casual").unwrap();
        assert_eq!(first.bundle_id, "com.apple.mail");
        assert_eq!(first.app_name, "Mail");

        let second = set_app_instruction(&state, "com.apple.mail", "Mail", "professional").unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.created_at, first.created_at);

        let listed = list_app_instructions(&state).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].style, "professional");

        assert!(set_app_instruction(&state, "com.apple.mail", "Mail", "shouting").is_err());
        assert!(set_app_instruction(&state, "", "Mail", "casual").is_err());

        delete_app_instruction(&state, &first.id).unwrap();
        assert!(list_app_instructions(&state).unwrap().is_empty());
    }

    #[test]
    fn app_dictionary_words_are_grouped_by_app() {
        let state = DbState::in_memory();
        let zed = add_app_dictionary_word(&state, "dev.zed.Zed", "Zed", "GPUI").unwrap();
        add_app_dictionary_word(&state, "com.apple.mail", "Mail", "VoxFusion").unwrap();
        add_app_dictionary_word(&state, "dev.zed.Zed", "Zed", "Rust").unwrap();

        let dictionaries = list_app_dictionaries(&state).unwrap();
        let apps: Vec<(&str, usize)> = dictionaries
            .iter()
            .map(|dictionary| (dictionary.app_name.as_str(), dictionary.words.len()))
            .collect();
        assert_eq!(apps, [("Mail", 1), ("Zed", 2)]);

        update_app_dictionary_word(&state, &zed.id, "gpui-kit").unwrap();
        assert_eq!(
            fetch_app_dictionary_words(&state.conn.lock().unwrap(), "dev.zed.Zed")
                .map(|words| words.contains("gpui-kit")),
            Some(true)
        );

        delete_app_dictionary_word(&state, &zed.id).unwrap();
        delete_app_dictionary(&state, "com.apple.mail").unwrap();
        let dictionaries = list_app_dictionaries(&state).unwrap();
        assert_eq!(dictionaries.len(), 1);
        assert_eq!(dictionaries[0].words[0].word, "Rust");
    }
}
