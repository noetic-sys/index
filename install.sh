#!/bin/sh
set -e

REPO="noetic-sys/index"
BINARY="idx"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) OS="darwin" ;;
  linux) OS="linux" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH="amd64" ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ASSET_NAME="idx-${OS}-${ARCH}"

# Get latest release
LATEST=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)

if [ -z "$LATEST" ]; then
  echo "Failed to get latest release"
  exit 1
fi

echo "Installing idx ${LATEST} for ${OS}-${ARCH}..."

# Download and extract
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/${ASSET_NAME}.tar.gz"
TMP_DIR=$(mktemp -d)
curl -sL "$DOWNLOAD_URL" | tar -xz -C "$TMP_DIR"

# Install
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP_DIR/$ASSET_NAME" "$INSTALL_DIR/$BINARY"
else
  echo "Need sudo to install to $INSTALL_DIR"
  sudo mv "$TMP_DIR/$ASSET_NAME" "$INSTALL_DIR/$BINARY"
fi

chmod +x "$INSTALL_DIR/$BINARY"
rm -rf "$TMP_DIR"

echo "Installed idx to $INSTALL_DIR/$BINARY"
echo "Run 'idx --help' to get started"
