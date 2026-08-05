use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Stdio, exit},
    str,
};

/// Logical name passed to bindgen for the in-memory wrapper. Bindgen needs a
/// `header_name` for diagnostics; this string never touches disk.
const WRAPPER_NAME: &str = "wrapper.h";

const LLVM_MAJOR_VERSION: usize = 22;

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error);
        exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    let link_mode = detect_link_mode();

    if !cfg!(feature = "no-version-check") {
        let version = llvm_config("--version", &link_mode)?;

        if !version.starts_with(&format!("{LLVM_MAJOR_VERSION}.")) {
            return Err(format!(
                "failed to find correct version ({LLVM_MAJOR_VERSION}.x.x) of llvm-config (found {version})"
            )
            .into());
        }
    }

    let directory = llvm_config("--libdir", &link_mode)?;
    println!("cargo:rustc-link-search={directory}");

    match link_mode {
        LinkMode::Static => {
            // `llvm-config --libnames` never reports the `MLIRCAPI*`
            // libraries at all (verified directly: `--libnames` output has
            // no `CAPI` entry) -- this disk scan is the *only* thing that
            // picks them up, by matching the raw filenames MLIR's own build
            // actually produces. On Unix that's `libMLIR*.a`; on Windows
            // it's `MLIR*.lib`, no `lib` prefix at all (verified: 0 of 389
            // `MLIR*` files in the install's `lib/` start with `lib`) --
            // `name.starts_with("libMLIR")` alone silently matches nothing
            // there, so every one of these libraries (including
            // `MLIRCAPIIR`, whose `mlirFloat8E3M4TypeGet`/etc. melior itself
            // calls) never reached the linker at all (found by direct
            // testing: an `LNK2019 unresolved external symbol` naming
            // exactly those functions, with zero mention of `MLIRCAPIIR` in
            // the actual link command).
            for entry in fs::read_dir(&directory)? {
                if let Some(name) = entry?.path().file_name().and_then(OsStr::to_str)
                    && (name.starts_with("libMLIR") || name.starts_with("MLIR"))
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
            // On Windows, `llvm-config --system-libs` reports names with
            // their own `.lib` suffix already (e.g. `psapi.lib`) -- unlike
            // Linux's bare `-lfoo` form, which the `trim_start_matches("-l")`
            // above already handles. Passed through as-is, Cargo's own
            // `cargo:rustc-link-lib` appends the platform's library suffix a
            // *second* time, producing a nonexistent `psapi.lib.lib` at link
            // time (found by direct testing).
            let flag = flag.strip_suffix(".lib").unwrap_or(flag);
            println!("cargo:rustc-link-lib={flag}");
        }
    }

    if let Some(name) = get_system_libcpp() {
        println!("cargo:rustc-link-lib={name}");
    }

    let include_dir = llvm_config("--includedir", &link_mode)?;
    let wrapper_contents = generate_wrapper_contents(&include_dir)?;

    let bindings = bindgen::builder()
        .header_contents(WRAPPER_NAME, &wrapper_contents)
        .clang_arg(format!("-I{include_dir}"))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap();

    // Scoped to MSVC specifically, not run unconditionally on every target:
    // this normalization has only been verified against the four enums
    // melior's own source actually exercises (`MlirWalkOrder`,
    // `MlirDiagnosticSeverity`, `MlirGreedyRewriteStrictness`,
    // `MlirGreedySimplifyRegionLevel`) -- there's no proof every other
    // `Mlir`-prefixed enum in the C API needs the same treatment, and if one
    // ever turned out to genuinely want `i32` (with Clang already inferring
    // `c_int` for it on non-MSVC targets too, i.e. already working there),
    // an unconditional normalization would silently break that currently-
    // correct case. Gating to the one target where the mismatch is actually
    // confirmed keeps this fix unable to change anything on Linux/macOS at
    // all, matching the same `CARGO_CFG_TARGET_ENV == "msvc"` precedent
    // `get_system_libcpp` above already uses.
    let text = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        normalize_enum_signedness(&bindings.to_string())
    } else {
        bindings.to_string()
    };
    fs::write(Path::new(&env::var("OUT_DIR")?).join("bindings.rs"), text)?;

    Ok(())
}

/// C leaves an `enum`'s underlying integer type implementation-defined --
/// Clang (used internally by bindgen) infers it per-target, and for these
/// particular MLIR C API enums (small, all-non-negative value sets) it
/// infers a *signed* `c_int` when targeting `-pc-windows-msvc`, but an
/// *unsigned* one on the Linux/macOS targets melior's own hand-written Rust
/// source is developed and tested against -- which hardcodes `u32`
/// throughout, breaking the build on Windows specifically (found by direct
/// testing: `MlirWalkOrder`/`MlirDiagnosticSeverity`/
/// `MlirGreedyRewriteStrictness` all came back `c_int` here). Safe to
/// normalize post-hoc: an `enum` parameter passed by value across an
/// `extern "C"` boundary has the same 4-byte representation whether Rust
/// calls it `i32` or `u32` -- only the *type-checker's* view of signedness
/// changes, not the actual C ABI.
///
/// Detects bindgen's own "Consts" enum-codegen shape structurally (a
/// `pub type NAME = ::std::os::raw::c_int;` alias immediately followed,
/// somewhere below, by at least one `pub const _: NAME = _;` using it) --
/// not by name-matching against a fixed list -- so any *MLIR* C API enum
/// this same issue affects, including ones no example here has exercised
/// yet, gets normalized the same way. Deliberately scoped two ways, both
/// required: the alias name must start with `Mlir` (the wrapper transitively
/// pulls in some plain `llvm-c/*.h` headers too, via `mlir-c`'s own
/// includes -- their enums are a different C API, not what this issue is
/// about, and at least one of them, `LLVMAttributeFunctionIndex`, genuinely
/// holds `-1` as a sentinel and would silently fail to compile if
/// unsigned-normalized); and none of its own `pub const` values may be
/// negative, a second, independent guard against exactly that same mistake.
fn normalize_enum_signedness(bindings: &str) -> String {
    // rustfmt-style output wraps a long `pub const NAME: Type = value;` onto
    // two lines (`pub const NAME:` / `    Type = value;`) once the name gets
    // long enough -- which every one of these actually does, being prefixed
    // with the enum's own name (`MlirGreedyRewriteStrictness_MLIR_GREEDY_
    // REWRITE_STRICTNESS_ANY_OP`, ...). Matching against a whitespace-
    // collapsed copy makes both detection passes below robust to that
    // wrapping without needing to special-case it explicitly.
    let flattened: String = bindings.split_whitespace().collect::<Vec<_>>().join(" ");

    let mut candidates = Vec::new();
    for line in bindings.lines() {
        let Some(rest) = line.trim_start().strip_prefix("pub type ") else { continue };
        let Some((name, ty)) = rest.split_once(" = ") else { continue };
        if name.starts_with("Mlir")
            && ty.trim_end_matches(';') == "::std::os::raw::c_int"
            && flattened.contains(&format!(": {name} ="))
        {
            candidates.push(name.to_string());
        }
    }

    let mut enum_alias_names = Vec::new();
    for name in candidates {
        let has_negative_value = flattened.contains(&format!(": {name} = -"));
        if !has_negative_value {
            enum_alias_names.push(name);
        }
    }

    let mut text = bindings.to_string();
    for name in &enum_alias_names {
        text = text.replace(
            &format!("pub type {name} = ::std::os::raw::c_int;"),
            &format!("pub type {name} = ::std::os::raw::c_uint;"),
        );
    }
    text
}

#[derive(Clone, Copy)]
enum LinkMode {
    Static,
    Shared,
}

/// Detect whether to link LLVM/MLIR statically or as shared libraries.
///
/// Checks in order:
/// 1. `MLIR_SYS_LINK_SHARED=1` env var forces shared
/// 2. Whether static libraries exist in the lib directory
/// 3. Falls back to `llvm-config --shared-mode`
fn detect_link_mode() -> LinkMode {
    if let Ok(val) = env::var("MLIR_SYS_LINK_SHARED")
        && val == "1"
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
    command.arg(link_flag).arg(argument).stderr(Stdio::null());
    run_command(command)
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

    command.arg(argument).stderr(Stdio::inherit());
    run_command(command)
}

fn run_command(mut command: Command) -> Result<String, Box<dyn Error>> {
    let output = command
        .output()
        .map_err(|error| format!("failed to run `{command:?}`: {error}"))?;

    if !output.status.success() {
        return Err(format!("failed to run `{command:?}`: {}", output.status).into());
    }

    Ok(str::from_utf8(&output.stdout)?.trim().into())
}

fn parse_static_lib_name(name: &str) -> Option<&str> {
    // Windows static libs have no `lib` prefix at all (`MLIRCAPIIR.lib`, not
    // `libMLIRCAPIIR.lib`) -- verified directly against the actual install's
    // `lib/` directory. `.lib` never appears as a Unix static-archive suffix
    // (that's always `.a`), so checking it first is unambiguous.
    if let Some(name) = name.strip_suffix(".lib") {
        Some(name)
    } else if let Some(name) = name.strip_prefix("lib") {
        name.strip_suffix(".a")
    } else {
        None
    }
}

fn parse_shared_lib_name(name: &str) -> Option<&str> {
    let name = name.strip_prefix("lib").unwrap_or(name);

    // Handle libFoo.so, libFoo.so.22, libFoo.dylib
    if let Some(pos) = name.find(".so") {
        Some(&name[..pos])
    } else if let Some(name) = name.strip_suffix(".dylib") {
        Some(name)
    } else {
        None
    }
}

/// Walk `{includedir}/mlir-c/` and build an in-memory list of `#include`s
/// covering every header. Returned as a `String` so it can be passed to
/// bindgen via `header_contents`, avoiding any on-disk wrapper file. A
/// disk-backed wrapper would have its mtime rewritten on every build,
/// which interacts badly with bindgen's `CargoCallbacks::rerun-if-changed`
/// tracking and forces cargo to rebuild this crate (and everything that
/// depends on it) on every invocation.
fn generate_wrapper_contents(include_dir: &str) -> Result<String, Box<dyn Error>> {
    let mlir_c_dir = Path::new(include_dir).join("mlir-c");
    let mut headers = Vec::new();
    collect_headers(&mlir_c_dir, &mlir_c_dir, &mut headers)?;
    headers.sort();

    let mut content = String::new();
    for header in &headers {
        content.push_str(&format!("#include <mlir-c/{header}>\n"));
    }
    Ok(content)
}

fn collect_headers(
    base: &Path,
    dir: &Path,
    headers: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip Bindings/ (Python bindings, not relevant for Rust FFI)
            if path.file_name().and_then(OsStr::to_str) == Some("Bindings") {
                continue;
            }
            collect_headers(base, &path, headers)?;
        } else if path.extension().and_then(OsStr::to_str) == Some("h") {
            let relative = path.strip_prefix(base)?;
            headers.push(relative.to_string_lossy().into_owned());
        }
    }
    Ok(())
}
