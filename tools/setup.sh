#!/bin/sh

set -e

[ -n "$CI" ]

llvm_version=23

case $RUNNER_OS in
Linux | macOS)
  brew install llvm@$llvm_version

  llvm_prefix=$(brew --prefix llvm@$llvm_version)

  (
    echo MLIR_SYS_${llvm_version}0_PREFIX=$llvm_prefix
    echo LD_LIBRARY_PATH=$llvm_prefix/lib:$LD_LIBRARY_PATH

    # For the discovery of the zstd library on macOS
    echo LIBRARY_PATH=$(brew --prefix)/lib
  ) >>$GITHUB_ENV
  ;;
Windows)
  llvm_prefix=$RUNNER_TEMP/llvm

  # The zlib and libxml2 development packages provide import libraries listed by llvm-config.
  $CONDA/Scripts/conda.exe create -y \
    -c conda-forge \
    -p $llvm_prefix \
    --override-channels \
    mlir=$llvm_version zlib libxml2-devel

  # `llvm-config` lists the zstd import library by its DLL name.
  # https://github.com/llvm/llvm-project/issues/134025
  (
    cd $llvm_prefix/Library/lib
    cp zstd.lib zstd.dll.lib
  )

  echo MLIR_SYS_${llvm_version}0_PREFIX=$llvm_prefix/Library >>$GITHUB_ENV
  echo $llvm_prefix/Library/bin >>$GITHUB_PATH
  ;;
esac
