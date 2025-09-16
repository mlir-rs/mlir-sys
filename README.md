# mlir-sys

[![GitHub Action](https://img.shields.io/github/actions/workflow/status/femtomc/mlir-sys/test.yaml?style=flat-square)](https://github.com/femtomc/mlir-sys/actions)
[![Crate](https://img.shields.io/crates/v/mlir-sys.svg?style=flat-square)](https://crates.io/crates/mlir-sys)
[![License](https://img.shields.io/github/license/femtomc/mlir-sys.svg?style=flat-square)](LICENSE)

Rust bindings to [the MLIR C API](https://mlir.llvm.org/docs/CAPI/).

## Versioning

The versioning scheme follows the LLVM version used, similar to `llvm-sys`. The crate version is the LLVM version multiplied by 10 with a patch version:

- `mlir-sys` 210.0.x is compatible with LLVM/MLIR 21.0.x
- `mlir-sys` 200.0.x is compatible with LLVM/MLIR 20.0.x

## Install

```sh
cargo add mlir-sys
```

This crate searches an `llvm-config` command on build and uses it to determine build configurations related to LLVM and MLIR. You can also use a `MLIR_SYS_210_PREFIX` environment variable to specify a custom directory of LLVM installation.

## License

[MIT](LICENSE)
