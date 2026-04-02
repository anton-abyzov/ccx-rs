#!/bin/sh
set -e

# CCX Installer — downloads the latest release binary for your platform

REPO="anton-abyzov/ccx-rs"
CCX_PATH_BLOCK_START="# >>> ccx path >>>"
CCX_PATH_BLOCK_END="# <<< ccx path <<<"

path_contains_dir() {
  case ":$PATH:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

choose_install_dir() {
  if [ -n "$CCX_INSTALL_DIR" ]; then
    printf '%s\n' "$CCX_INSTALL_DIR"
    return
  fi

  if command -v ccx >/dev/null 2>&1; then
    existing="$(command -v ccx)"
    existing_dir="$(dirname "$existing")"
    if [ -w "$existing_dir" ]; then
      printf '%s\n' "$existing_dir"
      return
    fi
  fi

  for candidate in "$HOME/.local/bin" "$HOME/bin"; do
    if path_contains_dir "$candidate"; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  printf '%s\n' "$HOME/.ccx/bin"
}

shell_profile_path() {
  case "${SHELL:-}" in
    */fish)
      printf '%s\n' "$HOME/.config/fish/config.fish"
      ;;
    */zsh)
      printf '%s\n' "$HOME/.zshrc"
      ;;
    */bash)
      for candidate in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile" "$HOME/.bashrc"; do
        if [ -f "$candidate" ]; then
          printf '%s\n' "$candidate"
          return
        fi
      done
      printf '%s\n' "$HOME/.bash_profile"
      ;;
    *)
      printf '%s\n' "$HOME/.profile"
      ;;
  esac
}

ensure_path_block() {
  profile="$(shell_profile_path)"
  mkdir -p "$(dirname "$profile")"
  [ -f "$profile" ] || : > "$profile"

  if grep -q "$CCX_PATH_BLOCK_START" "$profile" 2>/dev/null; then
    return 0
  fi

  {
    printf '\n%s\n' "$CCX_PATH_BLOCK_START"
    case "${SHELL:-}" in
      */fish)
        printf 'set -gx PATH "%s" $PATH\n' "$INSTALL_DIR"
        ;;
      *)
        printf 'export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        ;;
    esac
    printf '%s\n' "$CCX_PATH_BLOCK_END"
  } >> "$profile"

  PROFILE_UPDATED="$profile"
}

INSTALL_DIR="$(choose_install_dir)"
PROFILE_UPDATED=""

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
      arm64|aarch64) ARTIFACT="ccx-linux-arm64" ;;
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

if ! path_contains_dir "$INSTALL_DIR"; then
  ensure_path_block
fi

echo ""
echo "✓ CCX installed to $INSTALL_DIR/ccx"
if [ -n "$PROFILE_UPDATED" ]; then
  echo "✓ Updated shell profile: $PROFILE_UPDATED"
fi
echo ""
if ! path_contains_dir "$INSTALL_DIR"; then
  echo "Run in this shell:"
  case "${SHELL:-}" in
    */fish)
      echo "  set -gx PATH \"$INSTALL_DIR\" \$PATH"
      ;;
    *)
      echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
      echo "  hash -r"
      ;;
  esac
  echo ""
fi
echo "Get started:"
echo "  # Free (OpenRouter — no subscription needed):"
echo "  export OPENROUTER_API_KEY=\"your-key-from-openrouter.ai/keys\""
echo "  ccx --model nemotron"
echo ""
echo "  # Claude Max/Pro (auto-detected):"
echo "  ccx"
