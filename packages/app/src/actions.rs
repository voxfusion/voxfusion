//! Keyboard actions and their bindings.

use gpui_kit::{App, KeyBinding, NoAction, actions};

actions!(
    voxfusion,
    [
        /// Opens the settings.
        OpenSettings,
        /// Focuses the transcription search field on Home.
        FocusSearch,
        /// Closes the main window; the app keeps running in the menu bar.
        CloseWindow,
        Quit,
        CheckForUpdates,
        /// Moves the keyboard focus to the next control in the settings.
        NextControl,
        /// Moves the keyboard focus to the previous control in the settings.
        PreviousControl,
    ]
);

pub fn bind_keys(cx: &mut App) {
    #[cfg(target_os = "windows")]
    let app_bindings = [
        KeyBinding::new("ctrl-,", OpenSettings, Some("MainWindow")),
        KeyBinding::new("ctrl-f", FocusSearch, Some("Home")),
        KeyBinding::new("ctrl-w", CloseWindow, None),
        KeyBinding::new("ctrl-q", Quit, None),
    ];
    #[cfg(not(target_os = "windows"))]
    let app_bindings = [
        KeyBinding::new("cmd-,", OpenSettings, Some("MainWindow")),
        KeyBinding::new("cmd-f", FocusSearch, Some("Home")),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
    ];

    cx.bind_keys(app_bindings);
    cx.bind_keys([
        KeyBinding::new("tab", NextControl, Some("Settings")),
        KeyBinding::new("shift-tab", PreviousControl, Some("Settings")),
        // While a hotkey is being recorded the recorder gets Tab, which then
        // moves the focus nowhere.
        KeyBinding::new("tab", NoAction, Some("HotkeyRecorder")),
        KeyBinding::new("shift-tab", NoAction, Some("HotkeyRecorder")),
    ]);
}
