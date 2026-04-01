#!/bin/sh
set -e

# CCX Installer — downloads the latest release binary for your platform

REPO="anton-abyzov/ccx-rs"
INSTALL_DIR="${CCX_INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64|aarch64) ARTIFACT="ccx-macos-arm64" ;;
      x86_64)        ARTIFACT="ccx-macos-x64" ;;
      *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64) ARTIFACT="ccx-linux-x64" ;;
      *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Get latest release URL
LATEST_URL="https://github.com/$REPO/releases/latest/download/$ARTIFACT"

echo "Installing CCX ($OS $ARCH)..."
echo "Downloading from: $LATEST_URL"

# Download
TMP=$(mktemp)
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$LATEST_URL" -o "$TMP"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$LATEST_URL" -O "$TMP"
else
  echo "Error: curl or wget required"
  exit 1
fi

# Install
chmod +x "$TMP"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP" "$INSTALL_DIR/ccx"
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo mv "$TMP" "$INSTALL_DIR/ccx"
fi

echo ""
echo "CCX installed to $INSTALL_DIR/ccx"
echo ""
echo "Get started:"
echo "  # Free (OpenRouter — no subscription needed):"
echo "  export OPENROUTER_API_KEY=\"your-key-from-openrouter.ai/keys\""
echo "  ccx chat --provider openrouter --model \"nvidia/nemotron-3-super-120b-a12b:free\""
echo ""
echo "  # Claude Max/Pro (auto-detected):"
echo "  ccx chat"
