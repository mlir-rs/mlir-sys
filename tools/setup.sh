#!/bin/sh

set -e

llvm_version=19

brew update
brew install llvm@$llvm_version

llvm_prefix=$(brew --prefix llvm@$llvm_version)

echo MLIR_SYS_${llvm_version}0_PREFIX=$llvm_prefix >>$GITHUB_ENV
echo LD_LIBRARY_PATH=$llvm_prefix/lib:$LD_LIBRARY_PATH >>$GITHUB_ENV
