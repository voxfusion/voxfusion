<p align="center">
  <img src="packages/app/src-tauri/icons/icon.png" width="128" alt="VoxFusion icon">
</p>

<h1 align="center">VoxFusion</h1>

<p align="center">Voice to text for macOS that runs entirely on your Mac.</p>

<p align="center">
  <a href="https://voxfusion.io/download">Download</a>
  ·
  <a href="https://voxfusion.io">Website</a>
  ·
  <a href="CONTRIBUTING.md">Contributing</a>
  ·
  <a href="https://github.com/voxfusion/voxfusion/issues">Feedback &amp; ideas</a>
</p>

## Build from source

Install the Xcode command-line tools, [Rust](https://rustup.rs/) 1.85 or newer,
[Bun](https://bun.sh/), and [CMake](https://cmake.org/).

```sh
git clone https://github.com/voxfusion/voxfusion.git
cd voxfusion
bun install
cd packages/app
bun run dev
```

Whisper works out of the box. The Parakeet model also needs its engine, which
you build once with `packages/app/scripts/build-parakeet-engine.sh`; the
compile takes a while.

## License

VoxFusion is available under the [MIT](LICENSE) license.
