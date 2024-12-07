# mlir-sys

[![GitHub Action](https://img.shields.io/github/actions/workflow/status/femtomc/mlir-sys/test.yaml?style=flat-square)](https://github.com/femtomc/mlir-sys/actions)
[![Crate](https://img.shields.io/crates/v/mlir-sys.svg?style=flat-square)](https://crates.io/crates/mlir-sys)
[![License](https://img.shields.io/github/license/femtomc/mlir-sys.svg?style=flat-square)](LICENSE)

Rust bindings to [the MLIR C API](https://mlir.llvm.org/docs/CAPI/).

## Install

```sh
cargo add mlir-sys
```

This crate searches an `llvm-config` command on build and uses it to determine build configurations related to LLVM and MLIR. You can also use a `MLIR_SYS_190_PREFIX` environment variable to specify a custom directory of LLVM installation.

## License

[MIT](LICENSE)
