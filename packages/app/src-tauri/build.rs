fn main() {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    {
        // whisper.cpp's Metal backend contains Objective-C code with @available
        // checks, which reference ___isPlatformVersionAtLeast from compiler-rt.
        // We need to explicitly link libclang_rt.osx.a on macOS.
        let resource_dir = std::process::Command::new("clang")
            .arg("-print-resource-dir")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        let mut rt_path: Option<std::path::PathBuf> = None;

        if let Some(dir) = resource_dir {
            let candidate = std::path::Path::new(&dir).join("lib").join("darwin").join("libclang_rt.osx.a");
            if candidate.exists() {
                rt_path = Some(candidate);
            }
        }

        // Fallback: search common locations
        if rt_path.is_none() {
            let search_dirs = [
                "/Library/Developer/CommandLineTools/usr/lib/clang",
                "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/clang",
            ];
            for base in &search_dirs {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let candidate = entry.path().join("lib").join("darwin").join("libclang_rt.osx.a");
                        if candidate.exists() {
                            rt_path = Some(candidate);
                            break;
                        }
                    }
                }
                if rt_path.is_some() {
                    break;
                }
            }
        }

        if let Some(path) = rt_path {
            if let Some(dir) = path.parent() {
                println!("cargo:rustc-link-search=native={}", dir.display());
                println!("cargo:rustc-link-lib=static=clang_rt.osx");
            }
        } else {
            println!("cargo:warning=Could not find libclang_rt.osx.a. Linking may fail with undefined symbol ___isPlatformVersionAtLeast.");
        }
    }
}
