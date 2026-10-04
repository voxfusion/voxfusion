use std::path::{Path, PathBuf};

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    embed_info_plist();
    link_clang_runtime();
}

/// Embeds Info.plist in the executable, so a build run outside an app bundle
/// (`cargo run`) still carries the usage descriptions macOS requires before
/// it will show a permission prompt. A bundled app reads the same file from
/// `Contents/Info.plist`.
fn embed_info_plist() {
    println!("cargo:rerun-if-changed=macos/Info.plist");

    let template = std::fs::read_to_string("macos/Info.plist").expect("macos/Info.plist exists");
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets the package version");
    let plist = template.replace("__VERSION__", &version);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let path = out_dir.join("Info.plist");
    std::fs::write(&path, plist).expect("can write to OUT_DIR");

    println!(
        "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        path.display()
    );
}

/// whisper.cpp's Metal backend contains Objective-C code with `@available`
/// checks, which reference `___isPlatformVersionAtLeast` from compiler-rt.
/// That needs libclang_rt.osx.a linked explicitly.
fn link_clang_runtime() {
    let runtime = |clang_dir: &Path| {
        let candidate = clang_dir.join("lib/darwin/libclang_rt.osx.a");
        candidate.exists().then_some(candidate)
    };

    let from_clang = std::process::Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|dir| runtime(Path::new(dir.trim())));

    let from_toolchains = || {
        [
            "/Library/Developer/CommandLineTools/usr/lib/clang",
            "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/clang",
        ]
        .iter()
        .filter_map(|base| std::fs::read_dir(base).ok())
        .flat_map(|entries| entries.flatten())
        .find_map(|entry| runtime(&entry.path()))
    };

    match from_clang.or_else(from_toolchains) {
        Some(path) => {
            if let Some(dir) = path.parent() {
                println!("cargo:rustc-link-search=native={}", dir.display());
                println!("cargo:rustc-link-lib=static=clang_rt.osx");
            }
        }
        None => println!(
            "cargo:warning=Could not find libclang_rt.osx.a. Linking may fail with undefined symbol ___isPlatformVersionAtLeast."
        ),
    }
}
