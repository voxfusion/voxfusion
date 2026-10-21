//! Keyboard actions and their bindings.

use gpui_kit::{App, KeyBinding, NoAction, actions};

use crate::ui::hotkey_recorder::RECORDING_CONTEXT;

actions!(
    voxfusion,
    [
        /// Opens the settings.
        OpenSettings,
        /// Closes the main window; the app keeps running in the menu bar.
        CloseWindow,
        Quit,
        CheckForUpdates,
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-,", OpenSettings, Some("MainWindow")),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
        // Tab moves the focus, unless a hotkey is being recorded: then the
        // recorder gets the key.
        KeyBinding::new("tab", NoAction, Some(RECORDING_CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(RECORDING_CONTEXT)),
    ]);
}
