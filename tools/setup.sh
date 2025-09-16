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
    # For LLVM 21, we need to install from the development branch
    wget -qO - https://apt.llvm.org/llvm-snapshot.gpg.key | sudo apt-key add -
    
    # Determine Ubuntu codename
    codename=$(lsb_release -cs)
    
    # Add the appropriate repository
    echo "deb http://apt.llvm.org/${codename}/ llvm-toolchain-${codename}-${llvm_version} main" | sudo tee /etc/apt/sources.list.d/llvm-${llvm_version}.list
    echo "deb-src http://apt.llvm.org/${codename}/ llvm-toolchain-${codename}-${llvm_version} main" | sudo tee -a /etc/apt/sources.list.d/llvm-${llvm_version}.list
    
    sudo apt-get update
    
    # Install LLVM packages with MLIR support
    # Note: MLIR packages might have different naming conventions
    sudo apt-get install -y \
        llvm-${llvm_version} \
        llvm-${llvm_version}-dev \
        llvm-${llvm_version}-tools \
        clang-${llvm_version} \
        libclang-${llvm_version}-dev \
        liblld-${llvm_version}-dev \
        || {
        echo "Error: Failed to install LLVM ${llvm_version}"
        echo "Please check if LLVM ${llvm_version} is available for your Ubuntu version at https://apt.llvm.org/"
        exit 1
    }
    
    llvm_prefix=/usr/lib/llvm-${llvm_version}
else
    echo "Unsupported operating system: $(uname)"
    exit 1
fi

echo MLIR_SYS_${llvm_version}0_PREFIX=$llvm_prefix >>$GITHUB_ENV
echo LD_LIBRARY_PATH=$llvm_prefix/lib:$LD_LIBRARY_PATH >>$GITHUB_ENV
