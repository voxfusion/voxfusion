//! A way to ask a build what macOS allows it to do, without clicking through
//! onboarding. Permissions depend on how the app was bundled and signed
//! (Info.plist usage strings, entitlements, bundle identifier), so this is
//! how a build is checked before it ships:
//!
//! ```sh
//! open -W VoxFusion.app --args --permissions-report /tmp/report.json
//! open -W VoxFusion.app --args --permissions-report /tmp/report.json --request-microphone
//! open -W VoxFusion.app --args --permissions-report /tmp/report.json --request-audio-capture
//! ```
//!
//! It has to be launched as an app (`open`), not from a shell: macOS holds a
//! program started from a terminal to the terminal's permissions.

use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::backend::PermissionState;
use crate::backend::native::{self, permissions};

/// How long a request for System Audio Recording waits for an answer.
const AUDIO_CAPTURE_WAIT: Duration = Duration::from_secs(60);

#[derive(Serialize)]
struct Report {
    version: &'static str,
    microphone: PermissionState,
    accessibility: bool,
    /// System Audio Recording, which the muffle filter needs. Absent where
    /// the filter cannot run.
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_capture: Option<PermissionState>,
    /// The answer to the microphone prompt, when one was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    microphone_request: Option<bool>,
    /// Why System Audio Recording could not be asked for, when it was
    /// requested and could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_capture_error: Option<String>,
}

/// Writes the report and returns `true` when the arguments ask for it; the
/// process should then exit instead of starting the app.
pub fn run_if_asked() -> bool {
    let arguments: Vec<String> = std::env::args().collect();
    let Some(position) = arguments
        .iter()
        .position(|argument| argument == "--permissions-report")
    else {
        return false;
    };
    let Some(path) = arguments.get(position + 1).map(PathBuf::from) else {
        eprintln!("--permissions-report needs a file to write to");
        return true;
    };

    let report = |microphone_request, audio_capture_error| Report {
        version: env!("CARGO_PKG_VERSION"),
        microphone: permissions::microphone_permission(),
        accessibility: permissions::check_accessibility(),
        audio_capture: native::muffle_permission(),
        microphone_request,
        audio_capture_error,
    };
    let write = |report: Report| {
        let contents = serde_json::to_string_pretty(&report).expect("the report serializes");
        if let Err(error) = std::fs::write(&path, contents) {
            eprintln!("cannot write {}: {error}", path.display());
        }
    };

    // The state before asking is written first: asking blocks until the
    // prompt is answered, which a script watching for the prompt never does.
    write(report(None, None));

    let asked = |flag: &str| arguments.iter().any(|argument| argument == flag);

    if asked("--request-microphone") {
        let granted = permissions::request_microphone_permission();
        write(report(Some(granted), None));
    }

    if asked("--request-audio-capture") {
        // The same request the Audio settings make. Its prompt belongs to a
        // helper process that lives only as long as this one, so this one
        // stays until the prompt is answered.
        let error = native::try_request_muffle_permission().err();
        let deadline = Instant::now() + AUDIO_CAPTURE_WAIT;

        while error.is_none()
            && native::muffle_permission() == Some(PermissionState::Prompt)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(250));
        }
        write(report(None, error));
    }

    true
}
