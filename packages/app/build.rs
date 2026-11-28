use std::path::{Path, PathBuf};

fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            embed_info_plist();
            link_clang_runtime();
        }
        Ok("windows") => {
            embed_windows_resources();
        }
        _ => {}
    }
}

/// Embeds the application icon and Win32 `VS_VERSION_INFO` metadata into the
/// Windows executable so Explorer, shortcuts, Task Manager, and the installer
/// display VoxFusion's icon and version.
fn embed_windows_resources() {
    println!("cargo:rerun-if-changed=windows/icon.ico");
    println!("cargo:rerun-if-env-changed=VOXFUSION_REQUIRE_WINDOWS_ICON");

    let require = std::env::var("VOXFUSION_REQUIRE_WINDOWS_ICON").as_deref() == Ok("1");
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let icon_path = manifest_dir.join("windows").join("icon.ico");
    if !icon_path.is_file() {
        let msg = format!("Windows icon missing at {}", icon_path.display());
        if require {
            panic!("{msg}");
        }
        println!("cargo:warning={msg}");
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let major: u16 = std::env::var("CARGO_PKG_VERSION_MAJOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let minor: u16 = std::env::var("CARGO_PKG_VERSION_MINOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let patch: u16 = std::env::var("CARGO_PKG_VERSION_PATCH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let icon_rc_path = icon_path.to_string_lossy().replace('\\', "/");
    let rc_contents = format!(
        r#"1 ICON "{icon_rc_path}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "VoxFusion\0"
            VALUE "FileDescription", "VoxFusion\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "voxfusion-app\0"
            VALUE "OriginalFilename", "voxfusion-app.exe\0"
            VALUE "ProductName", "VoxFusion\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let rc_path = out_dir.join("voxfusion.rc");
    std::fs::write(&rc_path, rc_contents).expect("can write voxfusion.rc to OUT_DIR");

    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "msvc" {
        let Some(rc_exe) = find_msvc_rc() else {
            let msg = "Windows SDK rc.exe not found; executable icon and version metadata skipped";
            if require {
                panic!("{msg}");
            }
            println!("cargo:warning={msg}");
            return;
        };
        let res_path = out_dir.join("voxfusion.res");
        let status = std::process::Command::new(&rc_exe)
            .arg("/nologo")
            .arg("/fo")
            .arg(&res_path)
            .arg(&rc_path)
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("cargo:rustc-link-arg-bins={}", res_path.display());
            }
            other => {
                let msg = format!("rc.exe ({}) failed: {other:?}", rc_exe.display());
                if require {
                    panic!("{msg}");
                }
                println!("cargo:warning={msg}");
            }
        }
    } else {
        let Some(windres) = find_windres() else {
            let msg = "windres not found; executable icon and version metadata skipped";
            if require {
                panic!("{msg}");
            }
            println!("cargo:warning={msg}");
            return;
        };
        let obj_path = out_dir.join("voxfusion.res.o");
        let status = std::process::Command::new(&windres)
            .arg("-O")
            .arg("coff")
            .arg("-i")
            .arg(&rc_path)
            .arg("-o")
            .arg(&obj_path)
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("cargo:rustc-link-arg-bins={}", obj_path.display());
            }
            other => {
                let msg = format!("windres ({}) failed: {other:?}", windres.display());
                if require {
                    panic!("{msg}");
                }
                println!("cargo:warning={msg}");
            }
        }
    }
}

fn find_msvc_rc() -> Option<PathBuf> {
    if command_exists("rc.exe") {
        return Some(PathBuf::from("rc.exe"));
    }
    let arch_dir = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };
    let mut kits = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = std::env::var(var) {
            let bin_dir = PathBuf::from(base).join("Windows Kits").join("10").join("bin");
            if let Ok(entries) = std::fs::read_dir(&bin_dir) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join(arch_dir).join("rc.exe");
                    if candidate.is_file() {
                        kits.push(candidate);
                    }
                }
            }
            let flat = bin_dir.join(arch_dir).join("rc.exe");
            if flat.is_file() {
                kits.push(flat);
            }
        }
    }
    kits.sort();
    kits.pop()
}

fn find_windres() -> Option<PathBuf> {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "x86_64".to_string());
    let prefixed = format!("{arch}-w64-mingw32-windres");
    for candidate in [prefixed.as_str(), "windres"] {
        if command_exists(candidate) {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

fn command_exists(program: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| {
        let full = dir.join(program);
        full.is_file() || dir.join(format!("{program}.exe")).is_file()
    })
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
