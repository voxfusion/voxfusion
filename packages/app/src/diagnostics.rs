//! A way to ask a build what macOS allows it to do, without clicking through
//! onboarding. Permissions depend on how the app was bundled and signed
//! (Info.plist usage strings, entitlements, bundle identifier), so this is
//! how a build is checked before it ships:
//!
//! ```sh
//! open -W VoxFusion.app --args --permissions-report /tmp/report.json
//! open -W VoxFusion.app --args --permissions-report /tmp/report.json --request-microphone
//! ```
//!
//! It has to be launched as an app (`open`), not from a shell: macOS holds a
//! program started from a terminal to the terminal's permissions.

use serde::Serialize;
use std::path::PathBuf;

use crate::backend::PermissionState;
use crate::backend::native::permissions;

#[derive(Serialize)]
struct Report {
    version: &'static str,
    microphone: PermissionState,
    accessibility: bool,
    /// The answer to the microphone prompt, when one was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    microphone_request: Option<bool>,
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

    let report = |microphone_request| Report {
        version: env!("CARGO_PKG_VERSION"),
        microphone: permissions::microphone_permission(),
        accessibility: permissions::check_accessibility(),
        microphone_request,
    };
    let write = |report: Report| {
        let contents = serde_json::to_string_pretty(&report).expect("the report serializes");
        if let Err(error) = std::fs::write(&path, contents) {
            eprintln!("cannot write {}: {error}", path.display());
        }
    };

    // The state before asking is written first: asking blocks until the
    // prompt is answered, which a script watching for the prompt never does.
    write(report(None));

    if arguments
        .iter()
        .any(|argument| argument == "--request-microphone")
    {
        let granted = permissions::request_microphone_permission();
        write(report(Some(granted)));
    }

    true
}
