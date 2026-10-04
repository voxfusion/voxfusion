//! Anonymous usage analytics, sent to PostHog only while the setting allows
//! it.

use chrono::{DateTime, SecondsFormat, Utc};
use gpui_kit::{App, Global};
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::paths;
use crate::settings::SettingsStore;

const POSTHOG_KEY: &str = "phc_n9Bb8O4Xw0diLGriQ4FKjrfHvUnFWDYcwo0fdBbd36m";
const POSTHOG_HOST: &str = "https://eu.i.posthog.com";
const CAPTURE_PATH: &str = "/i/v0/e/";

/// Holds the random identifier that tells one installation's events from
/// another's. It is created with the first event that is sent, so opting out
/// before that leaves no identifier behind.
const DISTINCT_ID_FILE: &str = "analytics_id";

const SEND_TIMEOUT: Duration = Duration::from_secs(10);

struct Analytics {
    /// The setting, shared with the sending thread.
    enabled: Arc<AtomicBool>,
    events: mpsc::Sender<Event>,
}

impl Global for Analytics {}

struct Event {
    name: String,
    properties: Map<String, Value>,
    captured_at: DateTime<Utc>,
}

/// Starts analytics with the user's choice from the settings, and follows
/// the setting from then on. Events captured before this are dropped, since
/// the choice is not known yet.
pub fn init(cx: &mut App) {
    // Scripted runs must not show up in the product's statistics.
    #[cfg(feature = "fixture")]
    if std::env::var_os("VOXFUSION_FIXTURE").is_some() {
        return;
    }

    let settings = SettingsStore::entity(cx);
    let enabled = Arc::new(AtomicBool::new(
        settings.read(cx).settings().analytics_enabled,
    ));
    let (events, queue) = mpsc::channel();

    cx.observe(&settings, {
        let enabled = enabled.clone();
        move |settings, cx| {
            let allowed = settings.read(cx).settings().analytics_enabled;
            enabled.store(allowed, Ordering::Relaxed);
        }
    })
    .detach();

    let spawned = thread::Builder::new().name("analytics".into()).spawn({
        let enabled = enabled.clone();
        let endpoint = format!("{POSTHOG_HOST}{CAPTURE_PATH}");
        let distinct_id_file = paths::data_dir().join(DISTINCT_ID_FILE);

        move || send_events(queue, &enabled, &endpoint, &distinct_id_file)
    });
    if let Err(error) = spawned {
        log::debug!(target: "analytics", "thread_spawn_failed error={error}");
    }

    cx.set_global(Analytics { enabled, events });
}

/// Records `event` with `properties`.
pub fn capture(cx: &App, event: &str, properties: &[(&str, Value)]) {
    let Some(analytics) = cx.try_global::<Analytics>() else {
        return;
    };
    if !analytics.enabled.load(Ordering::Relaxed) {
        return;
    }

    let _ = analytics.events.send(Event {
        name: event.to_string(),
        properties: properties
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect(),
        captured_at: Utc::now(),
    });
}

/// Posts the queued events to `endpoint` one by one, off the main thread. A
/// failed event is dropped: analytics must never get in the app's way.
fn send_events(
    queue: mpsc::Receiver<Event>,
    enabled: &AtomicBool,
    endpoint: &str,
    distinct_id_file: &Path,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let client = reqwest::Client::builder().timeout(SEND_TIMEOUT).build();
    let (runtime, client) = match (runtime, client) {
        (Ok(runtime), Ok(client)) => (runtime, client),
        (Err(error), _) => {
            log::debug!(target: "analytics", "runtime_unavailable error={error}");
            return;
        }
        (_, Err(error)) => {
            log::debug!(target: "analytics", "client_unavailable error={error}");
            return;
        }
    };

    let mut distinct_id = None;

    for event in queue {
        // The user may have opted out while the event was waiting.
        if !enabled.load(Ordering::Relaxed) {
            continue;
        }

        let distinct_id = distinct_id.get_or_insert_with(|| load_distinct_id(distinct_id_file));
        let body = payload(&event, distinct_id).to_string();

        match runtime.block_on(post(&client, endpoint, body)) {
            Ok(status) if status.is_success() => {}
            Ok(status) => {
                log::debug!(target: "analytics", "capture_rejected event={} status={status}", event.name);
            }
            Err(error) => {
                log::debug!(target: "analytics", "capture_failed event={} error={error}", event.name);
            }
        }
    }
}

async fn post(
    client: &reqwest::Client,
    endpoint: &str,
    body: String,
) -> reqwest::Result<reqwest::StatusCode> {
    let response = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await?;

    Ok(response.status())
}

/// The body of PostHog's capture request for `event`.
fn payload(event: &Event, distinct_id: &str) -> Value {
    let mut properties = Map::new();
    properties.insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    // The user is never identified, so PostHog should not keep a person
    // profile for them.
    properties.insert("$process_person_profile".into(), false.into());
    properties.extend(event.properties.clone());

    json!({
        "api_key": POSTHOG_KEY,
        "event": event.name,
        "distinct_id": distinct_id,
        "timestamp": event.captured_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        "properties": properties,
    })
}

/// Reads the installation's identifier from `path`, creating it on first use.
fn load_distinct_id(path: &Path) -> String {
    let stored = std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| uuid::Uuid::parse_str(contents.trim()).ok());
    if let Some(id) = stored {
        return id.to_string();
    }

    let id = uuid::Uuid::new_v4().to_string();
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, &id));
    if let Err(error) = written {
        // The identifier then lasts for this run only.
        log::debug!(target: "analytics", "distinct_id_not_saved error={error}");
    }

    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::{Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::time::Instant;

    fn event(name: &str, properties: Value) -> Event {
        Event {
            name: name.into(),
            properties: properties.as_object().cloned().unwrap(),
            captured_at: Utc.with_ymd_and_hms(2026, 3, 9, 4, 5, 6).unwrap()
                + chrono::Duration::milliseconds(78),
        }
    }

    #[test]
    fn payload_is_a_posthog_capture_request() {
        let payload = payload(
            &event("settings_model_changed", json!({ "model": "parakeet-v3" })),
            "0c1f6f4e-52e5-4b0c-9d55-0f4b7d1f2a3b",
        );

        assert_eq!(
            payload,
            json!({
                "api_key": "phc_n9Bb8O4Xw0diLGriQ4FKjrfHvUnFWDYcwo0fdBbd36m",
                "event": "settings_model_changed",
                "distinct_id": "0c1f6f4e-52e5-4b0c-9d55-0f4b7d1f2a3b",
                "timestamp": "2026-03-09T04:05:06.078Z",
                "properties": {
                    "app_version": env!("CARGO_PKG_VERSION"),
                    "$process_person_profile": false,
                    "model": "parakeet-v3",
                },
            })
        );
    }

    #[test]
    fn every_event_carries_the_app_version() {
        let payload = payload(&event("app_opened", json!({})), "id");

        assert_eq!(
            payload["properties"]["app_version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn page_views_keep_their_posthog_property_names() {
        let payload = payload(
            &event("$pageview", json!({ "$current_url": "/dictionary" })),
            "id",
        );

        assert_eq!(payload["event"], "$pageview");
        assert_eq!(payload["properties"]["$current_url"], "/dictionary");
    }

    #[test]
    fn the_distinct_id_is_created_once_and_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile").join(DISTINCT_ID_FILE);

        let first = load_distinct_id(&path);
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);

        assert_eq!(load_distinct_id(&path), first);
    }

    #[test]
    fn a_damaged_distinct_id_is_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(DISTINCT_ID_FILE);
        std::fs::write(&path, "not an id").unwrap();

        let id = load_distinct_id(&path);
        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), id);
    }

    #[test]
    fn an_unwritable_profile_still_gets_an_id_for_the_run() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("file");
        std::fs::write(&blocker, "").unwrap();

        let id = load_distinct_id(&blocker.join(DISTINCT_ID_FILE));
        assert!(uuid::Uuid::parse_str(&id).is_ok());
    }

    /// Waits for a connection for a few seconds.
    fn accept(listener: &TcpListener) -> Option<TcpStream> {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);

        while Instant::now() < deadline {
            if let Ok((stream, _)) = listener.accept() {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return Some(stream);
            }
            thread::sleep(Duration::from_millis(5));
        }

        None
    }

    /// Reads one HTTP request, answers it, and returns its head and body.
    fn serve_one(listener: &TcpListener) -> (String, Value) {
        let mut stream = accept(listener).expect("no request arrived");
        let mut request = String::new();

        let (head, body) = loop {
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "the request ended early: {request}");
            request.push_str(std::str::from_utf8(&buffer[..read]).unwrap());

            let Some((head, body)) = request.split_once("\r\n\r\n") else {
                continue;
            };
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .expect("no content-length")
                .parse()
                .unwrap();
            if body.len() >= length {
                break (head.to_string(), serde_json::from_str(body).unwrap());
            }
        };

        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();

        (head, body)
    }

    #[test]
    fn sends_events_while_enabled_and_drops_them_after_opting_out() {
        let directory = tempfile::tempdir().unwrap();
        let distinct_id_file = directory.path().join(DISTINCT_ID_FILE);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}{CAPTURE_PATH}", listener.local_addr().unwrap());

        let enabled = Arc::new(AtomicBool::new(true));
        let (events, queue) = mpsc::channel();
        let sender = thread::spawn({
            let enabled = enabled.clone();
            let distinct_id_file = distinct_id_file.clone();
            move || send_events(queue, &enabled, &endpoint, &distinct_id_file)
        });

        events
            .send(event("default_style_changed", json!({ "style": "casual" })))
            .unwrap();
        let (head, body) = serve_one(&listener);

        assert!(head.starts_with("POST /i/v0/e/ HTTP/1.1\r\n"), "{head}");
        assert!(
            head.contains("content-type: application/json\r\n"),
            "{head}"
        );
        assert_eq!(body["event"], "default_style_changed");
        assert_eq!(body["properties"]["style"], "casual");
        assert_eq!(
            body["distinct_id"],
            std::fs::read_to_string(&distinct_id_file).unwrap()
        );

        enabled.store(false, Ordering::Relaxed);
        events.send(event("app_opened", json!({}))).unwrap();
        drop(events);
        sender.join().unwrap();

        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "an event was sent after opting out"
        );
    }

    #[test]
    fn opting_out_before_the_first_event_leaves_no_identifier() {
        let directory = tempfile::tempdir().unwrap();
        let distinct_id_file = directory.path().join(DISTINCT_ID_FILE);
        let (events, queue) = mpsc::channel();

        events.send(event("app_opened", json!({}))).unwrap();
        drop(events);
        send_events(
            queue,
            &AtomicBool::new(false),
            "http://127.0.0.1:9/",
            &distinct_id_file,
        );

        assert!(!distinct_id_file.exists());
    }
}
