//! The state of a dictation, and what the hotkeys, Escape and other keys
//! mean in each state.

use gpui_kit::SharedString;
use std::path::PathBuf;

use crate::backend::FrontmostApp;
use crate::paths::APP_IDENTIFIER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    /// Hands-free: one press of the hotkey starts, the next one ends.
    Toggle,
    /// Records for as long as the hotkey is held.
    Hold,
}

impl RecordingMode {
    pub fn name(self) -> &'static str {
        match self {
            RecordingMode::Toggle => "toggle",
            RecordingMode::Hold => "hold",
        }
    }
}

/// The app, and the site in it, that a dictation is typed into. They select
/// the dictionary and the style of the transcription.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Target {
    pub bundle_id: Option<String>,
    pub domain: Option<String>,
}

impl Target {
    /// The target for the frontmost app. The app's own windows are no target.
    pub fn of(frontmost: Option<FrontmostApp>) -> Self {
        match frontmost {
            Some(app) if !app.bundle_id.is_empty() && app.bundle_id != APP_IDENTIFIER => Self {
                bundle_id: Some(app.bundle_id),
                domain: app.domain,
            },
            _ => Self::default(),
        }
    }
}

/// A recording whose transcription failed and can be tried again.
#[derive(Debug, Clone, PartialEq)]
pub struct Retry {
    pub audio_path: PathBuf,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictationError {
    pub message: SharedString,
    pub retry: Option<Retry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start(RecordingMode),
    /// Ends the recording and transcribes it.
    Stop,
    /// Ends the recording and throws it away.
    Cancel,
    DismissError,
}

/// What the overlay shows of a dictation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pill {
    /// Whether there is anything to show at all.
    pub visible: bool,
    pub bars: bool,
    /// The cancel and confirm buttons of a hands-free recording.
    pub buttons: bool,
    pub error: bool,
    pub spinner: bool,
    /// Less padding at the ends, where round buttons sit.
    pub narrow: bool,
}

#[derive(Debug, Default)]
pub struct Dictation {
    pub recording: Option<RecordingMode>,
    /// The microphone is being opened.
    pub starting: bool,
    /// The recording is being ended, and transcribed unless it was cancelled.
    pub stopping: bool,
    /// A transcription is running.
    pub loading: bool,
    pub error: Option<DictationError>,
}

impl Dictation {
    fn busy(&self) -> bool {
        self.starting || self.stopping
    }

    pub fn toggle_pressed(&self) -> Option<Command> {
        if self.busy() {
            return None;
        }

        match self.recording {
            None => Some(Command::Start(RecordingMode::Toggle)),
            Some(RecordingMode::Toggle) => Some(Command::Stop),
            Some(RecordingMode::Hold) => None,
        }
    }

    pub fn hold_pressed(&self) -> Option<Command> {
        (!self.busy() && self.recording.is_none()).then_some(Command::Start(RecordingMode::Hold))
    }

    pub fn hold_released(&self) -> Option<Command> {
        (!self.busy() && self.recording == Some(RecordingMode::Hold)).then_some(Command::Stop)
    }

    /// A key other than the hotkey went down. Typing during hold-to-speak
    /// means the hotkey was part of a keyboard shortcut, not a dictation.
    pub fn other_key_pressed(&self) -> Option<Command> {
        (!self.busy() && self.recording == Some(RecordingMode::Hold)).then_some(Command::Cancel)
    }

    pub fn escape_pressed(&self) -> Command {
        if self.recording.is_some() {
            Command::Cancel
        } else {
            Command::DismissError
        }
    }

    pub fn pill(&self) -> Pill {
        let recording = self.recording.is_some();
        let hands_free = self.recording == Some(RecordingMode::Toggle);
        let error = self.error.is_some() && !recording && !self.loading;

        Pill {
            visible: recording || self.loading || self.error.is_some(),
            bars: recording || self.loading,
            buttons: hands_free && !self.loading,
            error,
            spinner: self.loading,
            narrow: hands_free || error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(mode: RecordingMode) -> Dictation {
        Dictation {
            recording: Some(mode),
            ..Dictation::default()
        }
    }

    fn error() -> DictationError {
        DictationError {
            message: "Recording failed".into(),
            retry: None,
        }
    }

    #[test]
    fn the_hands_free_hotkey_starts_and_ends_its_own_recordings() {
        assert_eq!(
            Dictation::default().toggle_pressed(),
            Some(Command::Start(RecordingMode::Toggle))
        );
        assert_eq!(
            recording(RecordingMode::Toggle).toggle_pressed(),
            Some(Command::Stop)
        );
        assert_eq!(recording(RecordingMode::Hold).toggle_pressed(), None);
    }

    #[test]
    fn the_hold_hotkey_records_between_press_and_release() {
        assert_eq!(
            Dictation::default().hold_pressed(),
            Some(Command::Start(RecordingMode::Hold))
        );
        assert_eq!(recording(RecordingMode::Hold).hold_pressed(), None);
        assert_eq!(recording(RecordingMode::Toggle).hold_pressed(), None);

        assert_eq!(
            recording(RecordingMode::Hold).hold_released(),
            Some(Command::Stop)
        );
        assert_eq!(recording(RecordingMode::Toggle).hold_released(), None);
        assert_eq!(Dictation::default().hold_released(), None);
    }

    #[test]
    fn hotkeys_wait_while_a_recording_starts_or_stops() {
        for dictation in [
            Dictation {
                starting: true,
                ..Dictation::default()
            },
            Dictation {
                stopping: true,
                ..Dictation::default()
            },
            Dictation {
                stopping: true,
                ..recording(RecordingMode::Hold)
            },
        ] {
            assert_eq!(dictation.toggle_pressed(), None);
            assert_eq!(dictation.hold_pressed(), None);
            assert_eq!(dictation.hold_released(), None);
            assert_eq!(dictation.other_key_pressed(), None);
        }
    }

    #[test]
    fn a_transcription_in_progress_does_not_block_the_next_recording() {
        let transcribing = Dictation {
            loading: true,
            ..Dictation::default()
        };

        assert_eq!(
            transcribing.hold_pressed(),
            Some(Command::Start(RecordingMode::Hold))
        );
    }

    #[test]
    fn another_key_cancels_only_hold_to_speak() {
        assert_eq!(
            recording(RecordingMode::Hold).other_key_pressed(),
            Some(Command::Cancel)
        );
        assert_eq!(recording(RecordingMode::Toggle).other_key_pressed(), None);
        assert_eq!(Dictation::default().other_key_pressed(), None);
    }

    #[test]
    fn escape_cancels_a_recording_and_otherwise_dismisses_the_error() {
        assert_eq!(
            recording(RecordingMode::Toggle).escape_pressed(),
            Command::Cancel
        );
        assert_eq!(
            recording(RecordingMode::Hold).escape_pressed(),
            Command::Cancel
        );
        assert_eq!(Dictation::default().escape_pressed(), Command::DismissError);
    }

    #[test]
    fn nothing_is_shown_between_dictations() {
        assert_eq!(Dictation::default().pill(), Pill::default());
        assert_eq!(
            Dictation {
                starting: true,
                ..Dictation::default()
            }
            .pill(),
            Pill::default()
        );
    }

    #[test]
    fn a_recording_shows_bars_and_hands_free_adds_its_buttons() {
        assert_eq!(
            recording(RecordingMode::Hold).pill(),
            Pill {
                visible: true,
                bars: true,
                ..Pill::default()
            }
        );
        assert_eq!(
            recording(RecordingMode::Toggle).pill(),
            Pill {
                visible: true,
                bars: true,
                buttons: true,
                narrow: true,
                ..Pill::default()
            }
        );
    }

    #[test]
    fn a_transcription_shows_bars_and_the_spinner() {
        let transcribing = Dictation {
            stopping: true,
            loading: true,
            ..Dictation::default()
        };

        assert_eq!(
            transcribing.pill(),
            Pill {
                visible: true,
                bars: true,
                spinner: true,
                ..Pill::default()
            }
        );
    }

    #[test]
    fn an_error_shows_once_recording_and_transcription_are_over() {
        let failed = Dictation {
            error: Some(error()),
            ..Dictation::default()
        };
        assert_eq!(
            failed.pill(),
            Pill {
                visible: true,
                error: true,
                narrow: true,
                ..Pill::default()
            }
        );

        let still_transcribing = Dictation {
            loading: true,
            error: Some(error()),
            ..Dictation::default()
        };
        assert!(!still_transcribing.pill().error);
        assert!(still_transcribing.pill().spinner);

        let recording_again = Dictation {
            error: Some(error()),
            ..recording(RecordingMode::Toggle)
        };
        assert!(!recording_again.pill().error);
        assert!(recording_again.pill().buttons);
    }

    #[test]
    fn the_buttons_go_when_a_retried_transcription_overlaps_a_recording() {
        let both = Dictation {
            loading: true,
            ..recording(RecordingMode::Toggle)
        };

        assert!(!both.pill().buttons);
        assert!(both.pill().spinner);
    }

    #[test]
    fn the_target_is_the_frontmost_app_unless_it_is_this_app() {
        let app = |bundle_id: &str, domain: Option<&str>| FrontmostApp {
            name: "App".into(),
            bundle_id: bundle_id.into(),
            url: None,
            domain: domain.map(str::to_string),
        };

        assert_eq!(
            Target::of(Some(app("com.apple.Safari", Some("github.com")))),
            Target {
                bundle_id: Some("com.apple.Safari".into()),
                domain: Some("github.com".into()),
            }
        );
        assert_eq!(
            Target::of(Some(app(APP_IDENTIFIER, None))),
            Target::default()
        );
        assert_eq!(
            Target::of(Some(app("", Some("github.com")))),
            Target::default()
        );
        assert_eq!(Target::of(None), Target::default());
    }
}
