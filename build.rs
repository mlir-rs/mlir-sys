use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::read_dir,
    path::Path,
    process::{Command, exit},
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

    let version = llvm_config("--version")?;

    if !version.starts_with(&format!("{LLVM_MAJOR_VERSION}.")) {
        return Err(format!(
            "failed to find correct version ({LLVM_MAJOR_VERSION}.x.x) of llvm-config (found {version})"
        )
        .into());
    }

    let lib_dir = llvm_config("--libdir")?;
    println!("cargo:rustc-link-search={lib_dir}");

    for entry in read_dir(lib_dir)? {
        if let Some(name) = entry?.path().file_name().and_then(OsStr::to_str)
            && name.starts_with("libMLIR")
            && let Some(name) = parse_archive_name(name)
        {
            println!("cargo:rustc-link-lib=static={name}");
        }
    }

    for name in llvm_config("--libnames")?.split(' ') {
        if let Some(name) = parse_archive_name(name) {
            println!("cargo:rustc-link-lib={name}");
        }
    }

    for flag in llvm_config("--system-libs")?.split(' ') {
        let flag = flag.trim_start_matches("-l");

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
        .clang_arg(format!("-I{}", llvm_config("--includedir")?))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap()
        .write_to_file(Path::new(&env::var("OUT_DIR")?).join("bindings.rs"))?;

    Ok(())
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

fn llvm_config(argument: &str) -> Result<String, Box<dyn Error>> {
    let prefix = env::var_os(format!("MLIR_SYS_{LLVM_MAJOR_VERSION}0_PREFIX"))
        .map(|path| Path::new(&path).join("bin"))
        .unwrap_or_default();

    let mut command = Command::new(prefix.join("llvm-config"));
    let output = command
        .arg("--link-static")
        .arg(argument)
        .output()
        .map_err(|e| format!("failed to run `{command:?}`: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "failed to run `{command:?}`: {}; stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        )
        .into());
    }

    Ok(str::from_utf8(&output.stdout)?.trim().into())
}

fn parse_archive_name(name: &str) -> Option<&str> {
    if let Some(name) = name.strip_prefix("lib") {
        name.strip_suffix(".a")
    } else {
        None
    }
}
