use std::{
    env,
    error::Error,
    path::Path,
    process::{Command, exit},
    str,
};

use llvm_bundler_rs::{dependency_graph::DependencyGraph, topological_sort::TopologicalSort};

const LLVM_MAJOR_VERSION: usize = 20;

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error);
        exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "bundled")]
    llvm_bundler_rs::bundler::bundle_cache()?;

    let version = llvm_config("--version")?;

    if !version.starts_with(&format!("{LLVM_MAJOR_VERSION}.",)) {
        return Err(format!(
            "failed to find correct version ({LLVM_MAJOR_VERSION}.x.x) of llvm-config (found {version})"
        )
        .into());
    }

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rustc-link-search={}", llvm_config("--libdir")?);

    let prefix =
        Path::new(&env::var(format!("MLIR_SYS_{LLVM_MAJOR_VERSION}0_PREFIX")).unwrap_or_default())
            .join("lib")
            .join("cmake")
            .join("mlir")
            .join("MLIRTargets.cmake");
    let path = DependencyGraph::from_cmake(prefix)?;
    let mlirlib = TopologicalSort::get_ordered_list(&path);

    for lib in mlirlib.iter().rev() {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    for flag in llvm_config("--libs")?.trim().split(' ') {
        let flag = flag.trim_start_matches("-l");
        println!("cargo:rustc-link-lib=static={flag}");
    }

    for flag in llvm_config("--system-libs")?.trim().split(' ') {
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
        .clang_arg("-I/usr/include")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap()
        .write_to_file(Path::new(&env::var("OUT_DIR")?).join("bindings.rs"))?;

    Ok(())
}

fn get_system_libcpp() -> Option<&'static str> {
    if cfg!(target_env = "msvc") {
        None
    } else if cfg!(target_os = "macos") {
        Some("c++")
    } else {
        Some("stdc++")
    }
}

fn llvm_config(argument: &str) -> Result<String, Box<dyn Error>> {
    let prefix = env::var(format!("MLIR_SYS_{LLVM_MAJOR_VERSION}0_PREFIX"))
        .map(|path| Path::new(&path).join("bin"))
        .unwrap_or_default();

    let llvm_config_exe = if cfg!(target_os = "windows") {
        "llvm-config.exe"
    } else {
        "llvm-config"
    };

    let path = prefix.join(llvm_config_exe);

    let output = Command::new(path)
        .arg("--link-static")
        .arg(argument)
        .output()?;

    if !output.status.success() {
        let stderr = output.stderr;
        eprintln!("{}", str::from_utf8(&stderr)?.trim().to_owned());
        exit(1);
    }

    let stdout = output.stdout;
    Ok(str::from_utf8(&stdout)?.trim().to_string())
}
