If you got some instingt useful for future agents, while working on task please write it down.

Use bun instead of Node.js, npm, pnpm, or vite.

Linux Tauri builds: `whisper-rs` must use the `metal` feature only on macOS (target-specific deps in `Cargo.toml`). Linux also needs `libstdc++-14-dev` for whisper.cpp/CMake. Headless smoke test: `Xvfb :99 -screen 0 1280x720x24 &` then `DISPLAY=:99 ./target/release/voxfusion-app`.
