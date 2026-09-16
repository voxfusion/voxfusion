# Linux / Omarchy delivery

Native QA performed on 16 September 2026 (Europe/Moscow).

## Deliverables

- [Linux x86_64 archive](voxfusion-linux-x86_64.tar.gz): extract, enter its directory, run `./install.sh`.
- [Desktop walkthrough](omarchy-walkthrough.mp4): 3:02, 1920×1080, H.264 video with AAC sample audio.
- [Dictated text in the native editor](omarchy-dictation.png).
- [Saved transcription history](omarchy-history.png).
- [Final packaged overlay](omarchy-recording.png).
- [Artifact checksums](SHA256SUMS), [build log](build.log) and [Rust test log](cargo-test.log).
- [Installation, dependencies and shortcuts](../../docs/linux.md).

The archive targets Omarchy/Arch with its native WebKitGTK libraries. It requires an x86_64 CPU with AVX2, FMA and F16C. Runtime dependencies and source-build instructions are in the Linux guide.

## What changed

Linux now builds without Apple Metal or macOS permissions. It uses local CPU Whisper, defaults to multilingual Whisper Base, checks Linux audio/desktop integration during onboarding, and provides Linux shortcut labels. Hyprland manages temporary global shortcuts and the floating overlay; `wtype` inserts the result into the focused app. XDG desktop entries identify applications for dictionaries and styles. Output muting restores its previous state on stop, cancel or normal app exit.

The package includes the native executable, a user-local installer, application menu entry, icon and documentation. CI now includes Linux Rust checks alongside macOS.

## Native desktop checks

| Check | Result |
| --- | --- |
| Linux onboarding, microphone enumeration, shortcut/privacy setup | Passed through the actual Tauri windows |
| Download Whisper Base from the model setup UI | Passed; file size and SHA-256 checked |
| Models, appearance, global dictionary and installed-app picker | Operated using native pointer/keyboard input |
| Ctrl+Alt+V toggle dictation with main window hidden | Passed; text inserted into a separate native GTK editor |
| Ctrl+Alt+B hold-to-speak | Passed; release ended capture and inserted the speech |
| Pure Wayland launch | Passed with `DISPLAY` removed and `GDK_BACKEND=wayland` |
| Destination focus while overlay is visible | Native editor remained focused |
| Hyprland configuration reload | Runtime shortcuts returned without editing user configuration |
| `--start` and `--cancel` control commands | Exercised through the private local socket |
| Optional media muting | Output changed from unmuted to muted during recording; cancel and quit restored unmuted state |
| Tray quit | App exited; zero owned Hyprland key bindings remained |
| Installer with a prefix containing spaces | Passed; quoted desktop entry launched the executable |

The recording shows real native application windows and real transcription. It uses the public [JFK speech sample from whisper.cpp](https://github.com/ggml-org/whisper.cpp/blob/master/samples/jfk.wav), routed from a temporary PipeWire source into VoxFusion's actual CPAL capture stream. This is reproducible speech input, not a live human microphone test. The system's default microphone was not changed. Only that sample's audio was included in the screen recording.

The walkthrough first shows models, hotkeys, appearance and dictionary configuration, then toggle dictation. At 2:25 it switches to a second take demonstrating hold-to-speak and saved history. Idle time between takes was removed. The second take runs the packaged pure-Wayland build. A subsequent overlay positioning adjustment uses the documented bottom/right anchors; the recorded takes precede that cosmetic adjustment.

The final hold-to-speak run processed approximately 11 seconds of speech in approximately 5 seconds after release. This is a single functional measurement, not a performance benchmark; an earlier run took longer while a release build competed for the CPU.

## Automated verification

- `bun run --filter @voxfusion/app build:linux`: native optimized release and archive produced.
- `cargo check`: passed on Linux.
- `cargo test -j4`: 12 tests passed, including Linux shortcut parsing, shell quoting and desktop-entry parsing.
- `bun run --filter @voxfusion/app typecheck`: passed.
- `bun run check`: passed, 121 files checked.
- `git diff --check`: passed.
- Native runtime dependencies resolved with `ldd`; Whisper's CMake cache confirms `GGML_NATIVE=OFF`, `GGML_AVX2=ON`, `GGML_FMA=ON`, `GGML_F16C=ON`.
- Exported MP4 inspected with `ffprobe` and fully decoded with FFmpeg without errors.

## Tested environment

Omarchy/Arch x86_64; Linux 6.17.9-arch1-1; Hyprland 0.52.2; WebKitGTK 2.50.3; PipeWire 1.4.9; glibc 2.42; wtype 0.4; Bun 1.3.3; Rust 1.92.0. QA used separate XDG data/config/cache directories and a separate workspace.

The final archive was installed and launched again without X11. Its bottom/right overlay position was verified. After QA, the app and test editor were closed, the temporary PipeWire devices were removed, and the original desktop workspace and unmuted output were restored.

## Remaining platform limits

- Browser website detection is unavailable on Linux; global and per-app dictionaries/styles remain available.
- Parakeet requires a separately supplied Linux CrispASR engine. The shipped Linux path uses Whisper.
- Hyprland 0.53+ rule syntax is implemented from its documentation, but only 0.52.2 received native QA here.
- X11, other Wayland compositors, `.deb`, `.rpm` and AppImage outputs were not validated on their target desktops/distributions. The Omarchy archive is the tested delivery.
- Linux updates require reinstalling a new archive. There is no Linux updater feed.
- A macOS native build was not available in this Linux environment; its Metal and permissions dependencies remain target-specific.
