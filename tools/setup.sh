#!/bin/sh

set -e

llvm_version=21

brew update

# For LLVM 21+, use the main llvm formula as versioned formulas aren't available yet
if [ $llvm_version -ge 21 ]; then
    brew install llvm
    llvm_prefix=$(brew --prefix llvm)
else
    brew install llvm@$llvm_version
    llvm_prefix=$(brew --prefix llvm@$llvm_version)
fi

echo MLIR_SYS_${llvm_version}0_PREFIX=$llvm_prefix >>$GITHUB_ENV
echo LD_LIBRARY_PATH=$llvm_prefix/lib:$LD_LIBRARY_PATH >>$GITHUB_ENV
