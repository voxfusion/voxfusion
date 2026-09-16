# VoxFusion on Linux (Omarchy / Arch)

VoxFusion runs as a native Tauri/WebKitGTK app on Linux. Omarchy's Hyprland session is the primary Wayland target. Speech recognition runs locally on the CPU; Linux does not require Apple Metal.

## Install a built archive

Install runtime dependencies from your usual Omarchy/Arch repositories:

```bash
sudo pacman -S --needed webkit2gtk-4.1 libappindicator-gtk3 alsa-lib libpulse xdotool wtype
```

Extract `voxfusion-linux-x86_64.tar.gz`, enter the extracted directory, and run `./install.sh`. This installs into `~/.local` and adds an application menu entry. No root access is needed for the app installation. `./install.sh /absolute/prefix` selects another prefix.

Launch **VoxFusion** from the application menu. Complete the microphone, desktop, shortcut, privacy and model setup. Linux defaults to multilingual **Whisper Base (~148 MB)** for CPU inference. **Whisper Large v3 Turbo (~1.5 GB)** offers better accuracy at a higher CPU/memory cost and can be downloaded in Settings → Models.

The x86_64 release uses ggml's generic AVX2 CPU build, not the build machine's native CPU flags. It requires AVX2, FMA and F16C. Build from source with suitable `GGML_*` flags for older CPUs.

## Use on Omarchy

- **Ctrl + Alt + V:** start dictation, press again to finish.
- **Ctrl + Alt + B:** hold while speaking, release to finish.
- **Escape:** cancel an active recording, if that key is available in Hyprland. The overlay also has a cancel button.
- Put the cursor in the destination app before starting. The overlay preserves keyboard focus.
- Closing the main window leaves dictation running in the tray. Use the tray menu to quit.
- Configure both shortcuts in onboarding or Settings. A letter, number, function or navigation key with modifiers is required on Linux.

VoxFusion installs temporary, described Hyprland bindings while running, restores them after a compositor config reload, and removes its own bindings on exit. Existing Hyprland bindings are not overwritten. If a chord is already assigned, choose another in Settings. The app does not edit `~/.config/hypr`.

The native control commands `voxfusion-app --toggle`, `--start`, `--stop` and `--cancel` can also be used in compositor scripts while VoxFusion is running. For other Wayland compositors, automatic shortcut setup is not supported yet. X11 uses Tauri's global shortcuts and Enigo text insertion; it has not received the same native QA as Hyprland.

## Build from source

```bash
sudo pacman -S --needed base-devel rust cmake clang pkgconf webkit2gtk-4.1 \
  libappindicator-gtk3 alsa-lib libpulse xdotool wtype
bun install --frozen-lockfile
bun run --filter @voxfusion/app build:linux
```

The archive and unpacked app are written to `packages/app/src-tauri/target/release/`. The build uses Bun and Cargo. `CARGO_BUILD_JOBS=4` is the script default to keep memory usage manageable.

For development: `bun run --filter @voxfusion/app dev`. Linux bundle configuration also enables `.deb`, `.rpm` and AppImage targets through Tauri. Those formats need to be built and tested on their intended distributions; the Arch archive is the primary Omarchy artifact.

For Ubuntu CI, install `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev libasound2-dev cmake clang` before `cargo check` / `cargo test` from `packages/app/src-tauri`.

## Capabilities and limits

- Recording uses ALSA through the system's PipeWire configuration. Select a working microphone in Settings; the desktop setup checks that the default input can open a configuration.
- Wayland text insertion uses `wtype`'s virtual keyboard protocol. Text is sent through stdin, preserving Unicode and avoiding shell expansion. The compositor must support this protocol (Hyprland does).
- Per-app styles and dictionaries use XDG desktop entries and Hyprland's active app identity. Browser website detection is unavailable on Linux; default and per-app rules still apply.
- Optional output muting uses `pactl` with PipeWire's PulseAudio compatibility service. It restores the original output's mute state, even if the default output changes.
- The bundled inference engine is Whisper. Parakeet is unavailable unless a separately built Linux CrispASR engine is supplied; macOS engine binaries cannot run on Linux.
- Linux release archives are updated by rebuilding/reinstalling. The macOS updater feed does not publish Linux updates.

Settings/history/models use the standard XDG directories under `io.voxfusion.app`. For isolated QA, set separate `XDG_DATA_HOME`, `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` before launch. The control socket is private to the current user under `$XDG_RUNTIME_DIR`.

If a dependency download returns 404, refresh the system using Omarchy's normal update workflow. Do not perform a partial Arch upgrade. During development on an older snapshot, exact installed-compatible packages can be obtained from the Arch archive.

References: [Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux), [Hyprland bindings](https://wiki.hypr.land/0.52.0/Configuring/Binds/), [wtype](https://github.com/atx/wtype).
