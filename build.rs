use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::read_dir,
    path::Path,
    process::{Command, Stdio, exit},
    str,
};

const LLVM_MAJOR_VERSION: usize = 21;

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error);
        exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");

    let link_mode = detect_link_mode();

    let version = llvm_config("--version", &link_mode)?;

    if !version.starts_with(&format!("{LLVM_MAJOR_VERSION}.")) {
        return Err(format!(
            "failed to find correct version ({LLVM_MAJOR_VERSION}.x.x) of llvm-config (found {version})"
        )
        .into());
    }

    let directory = llvm_config("--libdir", &link_mode)?;
    println!("cargo:rustc-link-search={directory}");

    match link_mode {
        LinkMode::Static => {
            for entry in read_dir(&directory)? {
                if let Some(name) = entry?.path().file_name().and_then(OsStr::to_str)
                    && name.starts_with("libMLIR")
                    && let Some(name) = parse_static_lib_name(name)
                {
                    println!("cargo:rustc-link-lib=static={name}");
                }
            }
        }
        LinkMode::Shared => {
            // With shared LLVM, MLIR is a single shared library.
            println!("cargo:rustc-link-lib=MLIR");
            // The C API is in a separate shared library.
            println!("cargo:rustc-link-lib=MLIR-C");
        }
    }

    for name in llvm_config("--libnames", &link_mode)?.split(' ') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        match link_mode {
            LinkMode::Static => {
                if let Some(name) = parse_static_lib_name(name) {
                    println!("cargo:rustc-link-lib={name}");
                }
            }
            LinkMode::Shared => {
                if let Some(name) = parse_shared_lib_name(name) {
                    println!("cargo:rustc-link-lib={name}");
                }
            }
        }
    }

    for flag in llvm_config("--system-libs", &link_mode)?.split(' ') {
        let flag = flag.trim().trim_start_matches("-l");

        if flag.is_empty() {
            continue;
        }

        if flag.starts_with('/') {
            // llvm-config returns absolute paths for dynamically linked libraries.
            let path = Path::new(flag);

            println!(
                "cargo:rustc-link-search={}",
                path.parent().unwrap().display()
            );
            println!(
                "cargo:rustc-link-lib={}",
                path.file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .trim_start_matches("lib")
            );
        } else {
            println!("cargo:rustc-link-lib={flag}");
        }
    }

    if let Some(name) = get_system_libcpp() {
        println!("cargo:rustc-link-lib={name}");
    }

    bindgen::builder()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", llvm_config("--includedir", &link_mode)?))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap()
        .write_to_file(Path::new(&env::var("OUT_DIR")?).join("bindings.rs"))?;

    Ok(())
}

#[derive(Clone, Copy)]
enum LinkMode {
    Static,
    Shared,
}

/// Detect whether to link LLVM/MLIR statically or as shared libraries.
///
/// Checks in order:
/// 1. `MLIR_SYS_LINK_SHARED` env var — "1" or "true" forces shared
/// 2. Whether static libraries exist in the lib directory
/// 3. Falls back to `llvm-config --shared-mode`
fn detect_link_mode() -> LinkMode {
    if let Ok(val) = env::var("MLIR_SYS_LINK_SHARED")
        && (val == "1" || val.eq_ignore_ascii_case("true"))
    {
        return LinkMode::Shared;
    }

    // Try static first — use --libnames which actually checks for libraries.
    if try_llvm_config("--libnames", "--link-static").is_ok() {
        return LinkMode::Static;
    }

    // Static failed, try shared.
    if try_llvm_config("--libnames", "--link-shared").is_ok() {
        return LinkMode::Shared;
    }

    // Default to static (will produce a clear error later).
    LinkMode::Static
}

fn get_system_libcpp() -> Option<&'static str> {
    if env::var("CARGO_CFG_TARGET_ENV").ok()? == "msvc" {
        None
    } else if env::var("CARGO_CFG_TARGET_VENDOR").ok()? == "apple" {
        Some("c++")
    } else {
        Some("stdc++")
    }
}

fn llvm_config_command() -> Command {
    let prefix = env::var_os(format!("MLIR_SYS_{LLVM_MAJOR_VERSION}0_PREFIX"))
        .map(|path| Path::new(&path).join("bin"))
        .unwrap_or_default();

    Command::new(prefix.join(if cfg!(target_os = "windows") {
        "llvm-config.exe"
    } else {
        "llvm-config"
    }))
}

fn try_llvm_config(argument: &str, link_flag: &str) -> Result<String, Box<dyn Error>> {
    let mut command = llvm_config_command();
    let output = command
        .arg(link_flag)
        .arg(argument)
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run `{command:?}`: {error}"))?;

    if !output.status.success() {
        return Err(format!("failed to run `{command:?}`: {}", output.status).into());
    }

    Ok(str::from_utf8(&output.stdout)?.trim().into())
}

fn llvm_config(argument: &str, link_mode: &LinkMode) -> Result<String, Box<dyn Error>> {
    let mut command = llvm_config_command();

    let link_flag = match link_mode {
        LinkMode::Static => "--link-static",
        LinkMode::Shared => "--link-shared",
    };

    command.arg(link_flag);

    // --ignore-libllvm only applies to static linking.
    if matches!(link_mode, LinkMode::Static) {
        command.arg("--ignore-libllvm");
    }

    let output = command
        .arg(argument)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("failed to run `{command:?}`: {error}"))?;

    if !output.status.success() {
        return Err(format!("failed to run `{command:?}`: {}", output.status).into());
    }

    Ok(str::from_utf8(&output.stdout)?.trim().into())
}

fn parse_static_lib_name(name: &str) -> Option<&str> {
    if let Some(name) = name.strip_prefix("lib") {
        name.strip_suffix(".a")
    } else {
        None
    }
}

fn parse_shared_lib_name(name: &str) -> Option<&str> {
    let name = name.strip_prefix("lib").unwrap_or(name);

    // Handle libFoo.so, libFoo.so.21, libFoo.dylib
    if let Some(pos) = name.find(".so") {
        Some(&name[..pos])
    } else if let Some(name) = name.strip_suffix(".dylib") {
        Some(name)
    } else {
        None
    }
}
