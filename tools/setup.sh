#!/bin/sh

set -e

llvm_version=21

# Detect the operating system
if [ "$(uname)" = "Darwin" ]; then
    # macOS
    brew update
    
    # For LLVM 21+, use the main llvm formula as versioned formulas aren't available yet
    if [ $llvm_version -ge 21 ]; then
        brew install llvm
        llvm_prefix=$(brew --prefix llvm)
    else
        brew install llvm@$llvm_version
        llvm_prefix=$(brew --prefix llvm@$llvm_version)
    fi
elif [ "$(uname)" = "Linux" ]; then
    # Linux (Ubuntu)
    # First try to install LLVM from apt
    wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key | sudo apt-key add -
    sudo apt-add-repository "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-${llvm_version} main" || true
    sudo apt-get update
    sudo apt-get install -y llvm-${llvm_version} llvm-${llvm_version}-dev
    
    llvm_prefix=/usr/lib/llvm-${llvm_version}
else
    echo "Unsupported operating system: $(uname)"
    exit 1
fi

echo MLIR_SYS_${llvm_version}0_PREFIX=$llvm_prefix >>$GITHUB_ENV
echo LD_LIBRARY_PATH=$llvm_prefix/lib:$LD_LIBRARY_PATH >>$GITHUB_ENV
