use base64::Engine;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::BufReader;
use std::panic;
use std::path::{Path, PathBuf};
use tauri::Manager;

use crate::handlers::db::DbState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    pub bundle_id: String,
    pub path: String,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmostApp {
    pub name: String,
    pub bundle_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInstruction {
    pub id: String,
    pub bundle_id: String,
    pub app_name: String,
    pub style: String,
    pub created_at: String,
    pub updated_at: String,
}

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
            ("en", "Pursuant to our prior discussion, I am writing to formally follow up on the third quarter strategic review. Please be advised that the executive committee has requested a comprehensive analysis at your earliest convenience. I would like to express my appreciation for your continued collaboration on this initiative."),
            ("ru", "Уважаемый коллега, в продолжение нашего предыдущего обсуждения я хотел бы официально запросить комплексный обзор стратегического плана за третий квартал. Прошу учесть, что исполнительный комитет ожидает детальный анализ в кратчайшие сроки. Благодарю вас за продуктивное сотрудничество в данном вопросе."),
            ("es", "En relación con nuestra conversación anterior, le escribo para realizar un seguimiento formal de la revisión estratégica del tercer trimestre. Le informo que el comité ejecutivo ha solicitado un análisis exhaustivo a la mayor brevedad posible. Le agradezco su continua colaboración en esta iniciativa."),
            ("fr", "Faisant suite à notre précédente discussion, je vous écris pour faire formellement le point sur l'examen stratégique du troisième trimestre. Je vous informe que le comité exécutif a demandé une analyse complète dans les meilleurs délais. Je vous remercie pour votre collaboration continue sur cette initiative."),
            ("de", "Im Anschluss an unsere vorherige Besprechung schreibe ich Ihnen, um offiziell auf die strategische Überprüfung des dritten Quartals zurückzukommen. Bitte beachten Sie, dass der Vorstand zum frühestmöglichen Zeitpunkt eine umfassende Analyse angefordert hat. Ich danke Ihnen für die fortgesetzte Zusammenarbeit an dieser Initiative."),
            ("it", "In seguito alla nostra discussione precedente, le scrivo per dare un seguito formale alla revisione strategica del terzo trimestre. La informo che il comitato esecutivo ha richiesto un'analisi approfondita nel più breve tempo possibile. La ringrazio per la continua collaborazione su questa iniziativa."),
            ("zh", "续我们之前的讨论，我谨此正式跟进第三季度的战略评估。请知悉，执行委员会已请求尽快进行全面分析。感谢您在此项工作中的持续协作。"),
        ],
    },
    StyleDef {
        key: "casual",
        prompts: &[
            ("en", "hey just wanted to ping you real quick. i'm gonna grab coffee in like ten you wanna come. honestly i'm so dead didn't sleep at all last night. anyway lemme know whenever. oh and that thing yeah i'm so down just hit me up."),
            ("ru", "привет короче хотел тебе быстренько написать. сейчас побегу за кофе минут через десять ты идёшь. честно я просто умираю не спал всю ночь. в общем напиши как сможешь. а ну и про то что мы вчера обсуждали да я полностью за пиши когда захочешь."),
            ("es", "oye solo quería escribirte rapidito. voy a por café en como diez minutos te apuntas. en serio estoy muerto no dormí nada anoche. en fin avísame cuando quieras. ah y eso de ayer sí me apunto escríbeme cuando puedas."),
            ("fr", "salut juste pour te ping rapidement. je vais prendre un café dans genre dix minutes tu viens. franchement je suis mort j'ai pas dormi du tout. enfin bref dis-moi quand tu veux. ah et ce truc d'hier ouais je suis chaud écris-moi quand tu peux."),
            ("de", "hey wollte dir nur kurz schreiben. ich geh in so zehn minuten kaffee holen kommst mit. ehrlich gesagt ich bin total fertig hab überhaupt nicht geschlafen. naja sag bescheid wann du zeit hast. ach und das thema von gestern bin total dabei schreib einfach wenn du willst."),
            ("it", "ciao volevo solo scriverti al volo. tra dieci minuti vado a prendere un caffè vieni anche tu. onestamente sono morto non ho dormito niente stanotte. comunque fammi sapere quando vuoi. ah e quella cosa di ieri sì ci sto scrivimi quando puoi."),
            ("zh", "嘿就是想快速跟你说一下。我大概十分钟后去喝咖啡你要来吗。说实话我快累死了昨晚根本没睡。反正随时告诉我。哦还有昨天那事我没问题随时找我。"),
        ],
    },
    StyleDef {
        key: "agents",
        prompts: &[
            ("en", "In src/lib/auth.ts refactor the handleLogin function to use async/await instead of promises. Add a useAuth hook in src/hooks/useAuth.ts that returns the current user and loading state. Update the LoginPage component to redirect to /dashboard when authenticated. Run bun run lint after the changes."),
            ("ru", "В файле src/lib/auth.ts перепиши функцию handleLogin на async/await вместо промисов. Добавь хук useAuth в src/hooks/useAuth.ts который возвращает текущего пользователя и состояние загрузки. Обнови компонент LoginPage чтобы он перенаправлял на /dashboard после авторизации. Запусти bun run lint после изменений."),
            ("es", "En src/lib/auth.ts refactoriza la función handleLogin para usar async/await en lugar de promesas. Agrega un hook useAuth en src/hooks/useAuth.ts que devuelva el usuario actual y el estado de carga. Actualiza el componente LoginPage para redirigir a /dashboard tras la autenticación. Ejecuta bun run lint después de los cambios."),
            ("fr", "Dans src/lib/auth.ts refactorise la fonction handleLogin pour utiliser async/await au lieu des promesses. Ajoute un hook useAuth dans src/hooks/useAuth.ts qui retourne l'utilisateur actuel et l'état de chargement. Mets à jour le composant LoginPage pour rediriger vers /dashboard après authentification. Exécute bun run lint après les modifications."),
            ("de", "In src/lib/auth.ts refaktoriere die handleLogin Funktion auf async/await statt Promises. Füge einen useAuth Hook in src/hooks/useAuth.ts hinzu der den aktuellen Benutzer und Ladezustand zurückgibt. Aktualisiere die LoginPage Komponente so dass nach der Authentifizierung auf /dashboard weitergeleitet wird. Führe bun run lint nach den Änderungen aus."),
            ("it", "In src/lib/auth.ts rifattorizza la funzione handleLogin per usare async/await invece di promesse. Aggiungi un hook useAuth in src/hooks/useAuth.ts che restituisce l'utente corrente e lo stato di caricamento. Aggiorna il componente LoginPage per reindirizzare a /dashboard dopo l'autenticazione. Esegui bun run lint dopo le modifiche."),
            ("zh", "在 src/lib/auth.ts 中将 handleLogin 函数重构为使用 async/await 而不是 promises。在 src/hooks/useAuth.ts 中添加 useAuth 钩子，返回当前用户和加载状态。更新 LoginPage 组件，在认证后重定向到 /dashboard。完成后运行 bun run lint。"),
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
    fallback_style: Option<&str>,
) -> String {
    if let Some(bid) = bundle_id.filter(|s| !s.is_empty()) {
        if let Ok(style) = conn.query_row(
            "SELECT style FROM app_instructions WHERE bundle_id = ?1",
            params![bid],
            |row| row.get::<_, String>(0),
        ) {
            return style;
        }
    }
    fallback_style
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_STYLE_KEY)
        .to_string()
}

pub fn run_migrations(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_instructions (
            id TEXT PRIMARY KEY,
            bundle_id TEXT NOT NULL UNIQUE,
            app_name TEXT NOT NULL,
            style TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_app_instructions_bundle_id ON app_instructions(bundle_id);",
    )
    .map_err(|e| format!("Failed to create app_instructions table: {}", e))?;
    Ok(())
}

fn read_bundle_id(plist: &plist::Dictionary) -> Option<String> {
    plist
        .get("CFBundleIdentifier")?
        .as_string()
        .map(|s| s.to_string())
}

fn read_display_name(plist: &plist::Dictionary, fallback: &str) -> String {
    if let Some(name) = plist.get("CFBundleDisplayName").and_then(|v| v.as_string()) {
        return name.to_string();
    }
    if let Some(name) = plist.get("CFBundleName").and_then(|v| v.as_string()) {
        return name.to_string();
    }
    fallback.trim_end_matches(".app").to_string()
}

fn read_info_plist(app_path: &Path) -> Option<plist::Dictionary> {
    let plist_path = app_path.join("Contents").join("Info.plist");
    let result = panic::catch_unwind(|| plist::Value::from_file(&plist_path).ok()).ok()??;
    result.into_dictionary()
}

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

#[tauri::command]
pub async fn list_installed_apps(
    app_handle: tauri::AppHandle,
) -> Result<Vec<InstalledApp>, String> {
    let home_dir = app_handle.path().home_dir().ok();
    tokio::task::spawn_blocking(move || {
        let mut results: Vec<InstalledApp> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        let mut dirs: Vec<PathBuf> = vec![PathBuf::from("/Applications")];
        if let Some(home) = home_dir {
            dirs.push(home.join("Applications"));
        }
        dirs.push(PathBuf::from("/System/Applications"));

        for dir in dirs {
            scan_apps_dir(&dir, &mut results, &mut seen);
        }

        results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(results)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn get_frontmost_app() -> Result<Option<FrontmostApp>, String> {
    tokio::task::spawn_blocking(|| {
        let result = panic::catch_unwind(|| {
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
            Some(FrontmostApp { name, bundle_id })
        })
        .unwrap_or(None);
        Ok(result)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub async fn get_frontmost_app() -> Result<Option<FrontmostApp>, String> {
    Ok(None)
}

#[tauri::command]
pub fn list_app_instructions(
    state: tauri::State<'_, DbState>,
) -> Result<Vec<AppInstruction>, String> {
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

fn normalize_style(style: &str) -> Result<&'static str, String> {
    STYLES
        .iter()
        .find(|s| s.key == style)
        .map(|s| s.key)
        .ok_or_else(|| format!("Invalid style: {}", style))
}

#[tauri::command]
pub fn set_app_instruction(
    state: tauri::State<'_, DbState>,
    bundle_id: String,
    app_name: String,
    style: String,
) -> Result<AppInstruction, String> {
    let bundle_id = bundle_id.trim().to_string();
    let app_name = app_name.trim().to_string();
    if bundle_id.is_empty() || app_name.is_empty() {
        return Err("bundle_id and app_name are required".to_string());
    }
    let style = normalize_style(style.as_str())?;
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

#[tauri::command]
pub fn delete_app_instruction(state: tauri::State<'_, DbState>, id: String) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM app_instructions WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

