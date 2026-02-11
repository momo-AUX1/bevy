use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn rustflags_has_cfg(cfg_name: &str) -> bool {
    let encoded = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    if !encoded.is_empty() {
        for part in encoded.split('\x1f') {
            if part.contains(cfg_name) {
                return true;
            }
        }
    }
    env::var("RUSTFLAGS")
        .unwrap_or_default()
        .contains(cfg_name)
}

fn pick_arch() -> String {
    if let Ok(arch) = env::var("CARGO_CFG_TARGET_ARCH") {
        return match arch.as_str() {
            "x86_64" => "x64".to_string(),
            "x86" | "i686" => "x86".to_string(),
            "aarch64" => "arm64".to_string(),
            _ if arch.starts_with("arm") => "arm".to_string(),
            _ => "x64".to_string(),
        };
    }

    let target = env::var("TARGET").unwrap_or_default().to_ascii_lowercase();
    if target.contains("aarch64") {
        return "arm64".to_string();
    }
    if target.contains("armv7") || target.contains("thumbv7") || target.contains("arm") {
        return "arm".to_string();
    }
    if target.contains("i686") || target.contains("i586") || target.contains("x86") {
        return "x86".to_string();
    }
    if target.contains("x86_64") || target.contains("amd64") {
        return "x64".to_string();
    }
    "x64".to_string()
}

fn parse_version(value: &str) -> Vec<u32> {
    value
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

fn max_version(versions: Vec<String>) -> Option<String> {
    let mut best: Option<(Vec<u32>, String)> = None;
    for v in versions {
        let key = parse_version(&v);
        if key.is_empty() {
            continue;
        }
        match &best {
            None => best = Some((key, v)),
            Some((best_key, _)) if key > *best_key => best = Some((key, v)),
            _ => {}
        }
    }
    best.map(|(_, v)| v)
}

fn windows_kits_root() -> Option<PathBuf> {
    if let Ok(dir) = env::var("WindowsSdkDir") {
        let path = PathBuf::from(dir);
        if path.exists() {
            return Some(path);
        }
    }

    let candidates = [
        env::var("ProgramFiles(x86)").ok(),
        env::var("ProgramFiles").ok(),
        env::var("ProgramW6432").ok(),
    ];

    for candidate in candidates.iter().flatten() {
        let path = Path::new(candidate).join("Windows Kits").join("10");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn find_live_sdk(arch: &str) -> Option<(PathBuf, PathBuf)> {
    let kits_root = windows_kits_root()?;
    let include_root = kits_root.join("Include");
    let versions = fs::read_dir(&include_root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    let version = max_version(versions)?;

    let lib_dir = kits_root.join("Lib").join(&version).join("um").join(arch);
    if !lib_dir.exists() {
        return None;
    }

    let union_metadata = kits_root.join("UnionMetadata").join(&version);
    if union_metadata.exists() {
        return Some((lib_dir, union_metadata));
    }

    let references = kits_root.join("References").join(&version);
    if references.exists() {
        return Some((lib_dir, references));
    }

    Some((lib_dir, PathBuf::new()))
}

fn sanitize_namespace(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        out.push_str("App");
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(__WINRT__)");
    println!("cargo:rerun-if-env-changed=WINDOWS_METADATA_PATH");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");

    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows-gnu") && !rustflags_has_cfg("__WINRT__") {
        panic!(
            "Missing --cfg __WINRT__ for TARGET={target}. Set RUSTFLAGS=--cfg __WINRT__ for UWP."
        );
    }

    let arch = pick_arch();
    let live_sdk = find_live_sdk(&arch);
    let metadata_override = env::var("WINDOWS_METADATA_PATH")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);

    let lib_dir = live_sdk.as_ref().map(|(lib_dir, _)| lib_dir.clone());
    let metadata_path = metadata_override.or_else(|| {
        live_sdk.as_ref().and_then(|(_, metadata)| {
            if metadata.as_os_str().is_empty() {
                None
            } else {
                Some(metadata.clone())
            }
        })
    });

    if let Some(path) = metadata_path {
        println!("cargo:rustc-env=WINDOWS_METADATA_PATH={}", path.display());
    }
    if let Some(path) = lib_dir.as_ref() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }

    println!("cargo:rustc-link-arg=-municode");
    println!("cargo:rustc-link-arg=-Wl,-subsystem,windows");
    println!("cargo:rustc-link-arg=-DUNICODE");
    println!("cargo:rustc-link-arg=-D_UNICODE");
    println!("cargo:rustc-link-arg=-DWIN32_LEAN_AND_MEAN");
    println!("cargo:rustc-link-arg=-DWINRT_LEAN_AND_MEAN");
    println!("cargo:rustc-link-lib=windowsapp");
    println!("cargo:rustc-link-lib=runtimeobject");
    println!("cargo:rustc-link-lib=ole32");
    println!("cargo:rustc-link-lib=shell32");

    if let Some(path) = lib_dir.as_ref() {
        let winstore_a = path.join("libwinstorecompat.a");
        let winstore_lib = path.join("winstorecompat.lib");
        if winstore_a.exists() || winstore_lib.exists() {
            println!("cargo:rustc-link-lib=winstorecompat");
        }
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let target_dir = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("target"));
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let host_target = env::var("HOST").unwrap_or_default();
    let use_triple_dir = !target.is_empty() && target != host_target;
    let exe_dir = if use_triple_dir {
        target_dir.join(&target).join(&profile)
    } else {
        target_dir.join(&profile)
    };
    let _ = fs::create_dir_all(&exe_dir);

    let images_src = manifest_dir.join("Images");
    if images_src.exists() {
        let _ = copy_dir_all(&images_src, &exe_dir.join("Images"));
    }

    let deps_bin = manifest_dir.join("deps").join("bin");
    if deps_bin.exists() {
        let _ = copy_dir_all(&deps_bin, &exe_dir);
    }

    let manifest_in = manifest_dir.join("AppxManifest.in");
    if manifest_in.exists() {
        if let Ok(mut content) = fs::read_to_string(&manifest_in) {
            let package = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "App".to_string());
            let publisher =
                env::var("MINGW_UWP_PUBLISHER").unwrap_or_else(|_| "CN=Unknown".to_string());
            let namespace = sanitize_namespace(&package);
            content = content.replace("winit_test", &package);
            content = content.replace("CN=Unknown", &publisher);
            content = content.replace("winit_test", &namespace);
            content = content.replace("@APPX_ARCHITECTURE@", &arch);
            let _ = fs::write(exe_dir.join("AppxManifest.xml"), &content);
        }
    }
}
