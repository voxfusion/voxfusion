use enigo::{Enigo, Keyboard, Settings};

use crate::platform::main_thread;

/// Types `text` into the focused app as keyboard input, which macOS only
/// lets through with the Accessibility permission.
pub fn type_text(text: &str) -> Result<(), String> {
    // On the main thread: enigo's macOS backend looks keys up through Text
    // Input Sources, which abort the process on any other thread. `text`
    // avoids those lookups today, but only by how enigo happens to be
    // written.
    main_thread::run(|| {
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        enigo.text(text).map_err(|e| e.to_string())?;
        Ok(())
    })
}
