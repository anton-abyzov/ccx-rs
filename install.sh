#!/bin/sh
set -e

# CCX Installer — downloads the latest release binary for your platform

REPO="anton-abyzov/ccx-rs"
INSTALL_DIR="${CCX_INSTALL_DIR:-$HOME/.ccx/bin}"

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
  MINGW*|MSYS*|CYGWIN*)
    ARTIFACT="ccx-windows-x64.exe"
    INSTALL_DIR="${CCX_INSTALL_DIR:-$HOME/bin}"
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

# Install — no sudo needed (installs to ~/.ccx/bin/)
chmod +x "$TMP"
mkdir -p "$INSTALL_DIR"
mv "$TMP" "$INSTALL_DIR/ccx"

# Add to PATH if not already there
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo ""
  echo "Add CCX to your PATH (add to ~/.zshrc or ~/.bashrc):"
  echo ""
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  echo ""
  # Try to add automatically
  for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
    if [ -f "$rc" ] && ! grep -q "$INSTALL_DIR" "$rc"; then
      echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$rc"
      echo "Added to $rc"
      break
    fi
  done
fi

echo ""
echo "✓ CCX installed to $INSTALL_DIR/ccx"
echo ""
echo "Get started:"
echo "  # Free (OpenRouter — no subscription needed):"
echo "  export OPENROUTER_API_KEY=\"your-key-from-openrouter.ai/keys\""
echo "  ccx --model nemotron"
echo ""
echo "  # Claude Max/Pro (auto-detected):"
echo "  ccx"
