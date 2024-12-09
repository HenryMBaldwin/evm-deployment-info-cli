#!/bin/bash

set -e

# Determine OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Convert architecture names
case $ARCH in
    x86_64)
        ARCH="amd64"
        ;;
    aarch64|arm64)
        ARCH="arm64"
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Set binary name based on OS
case $OS in
    linux)
        BINARY_NAME="evm-deployment-info-linux-$ARCH"
        ;;
    darwin)
        BINARY_NAME="evm-deployment-info-macos-$ARCH"
        ;;
    *)
        echo "Unsupported operating system: $OS"
        exit 1
        ;;
esac

# Get latest release version from GitHub
LATEST_RELEASE=$(curl -s https://api.github.com/repos/henrymbaldwin/evm-deployment-info-cli/releases/latest | grep -o '"tag_name": *"[^"]*"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

# Create installation directory
INSTALL_DIR="/usr/local/bin"
sudo mkdir -p "$INSTALL_DIR"

echo "Downloading $BINARY_NAME $LATEST_RELEASE..."
# Download the binary
sudo curl -L "https://github.com/henrymbaldwin/evm-deployment-info-cli/releases/download/$LATEST_RELEASE/$BINARY_NAME" -o "$INSTALL_DIR/evm-deployment-info"

# Make it executable
sudo chmod +x "$INSTALL_DIR/evm-deployment-info"

echo "evm-deployment-info has been installed to $INSTALL_DIR/evm-deployment-info"
echo "You can now run 'evm-deployment-info' from anywhere in your terminal." 