fn main() {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    {
        // whisper.cpp's Metal backend contains Objective-C code with @available
        // checks, which reference ___isPlatformVersionAtLeast from compiler-rt.
        // We need to explicitly link libclang_rt.osx.a on macOS when building
        // for targets that may not automatically include it.
        let sdk_path = std::process::Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        let clang_bin = std::process::Command::new("xcrun")
            .args(["--find", "clang"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        let clang_version = std::process::Command::new("clang")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines().next().and_then(|line| {
                    line.split("version").nth(1).map(|v| {
                        v.trim().split_whitespace().next().unwrap_or("").to_string()
                    })
                })
            });

        let mut rt_dir: Option<std::path::PathBuf> = None;

        // Search in SDK
        if let Some(ref sdk) = sdk_path {
            let search = format!("{}/usr/lib/clang", sdk);
            if let Ok(out) = std::process::Command::new("find")
                .args([&search, "-name", "libclang_rt.osx.a", "-print", "-quit"])
                .output()
            {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    rt_dir = std::path::Path::new(&p).parent().map(|d| d.to_path_buf());
                }
            }
        }

        // Search in toolchain
        if rt_dir.is_none() {
            if let (Some(ref bin), Some(ref ver)) = (clang_bin, clang_version) {
                let candidate = std::path::Path::new(bin)
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.join("lib").join("clang").join(ver).join("lib").join("darwin"));
                if let Some(ref c) = candidate {
                    if c.join("libclang_rt.osx.a").exists() {
                        rt_dir = Some(c.clone());
                    }
                }
            }
        }

        if let Some(dir) = rt_dir {
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        } else {
            println!("cargo:warning=Could not find libclang_rt.osx.a. If linking fails with undefined symbol ___isPlatformVersionAtLeast, ensure Xcode is installed.");
        }
    }
}
