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

active_ccx_path() {
  old_ifs=$IFS
  IFS=:
  set -- $PATH
  IFS=$old_ifs

  for dir do
    [ -n "$dir" ] || continue
    candidate="$dir/ccx"
    if [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
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

binary_version() {
  "$1" --version 2>/dev/null | awk 'NR==1 { print $2 }'
}

reconcile_existing_installs() {
  installed="$INSTALL_DIR/ccx"
  installed_version="$(binary_version "$installed")"
  seen_paths=""
  synced_paths=""
  warning_lines=""

  old_ifs=$IFS
  IFS=:
  set -- $PATH
  IFS=$old_ifs

  for dir do
    [ -n "$dir" ] || continue
    candidate="$dir/ccx"
    [ -f "$candidate" ] || continue

    case ":$seen_paths:" in
      *":$candidate:"*) continue ;;
    esac
    seen_paths="${seen_paths}:$candidate"

    [ "$candidate" = "$installed" ] && continue

    candidate_version="$(binary_version "$candidate")"
    [ "$candidate_version" = "$installed_version" ] && continue

    candidate_dir="$(dirname "$candidate")"
    if [ -w "$candidate_dir" ]; then
      cp "$installed" "$candidate"
      chmod +x "$candidate"
      synced_paths="${synced_paths}\n  $candidate"
    else
      if [ -n "$candidate_version" ]; then
        warning_lines="${warning_lines}\n  Stale ccx binary on PATH: $candidate (v$candidate_version); remove it or replace it with v$installed_version."
      else
        warning_lines="${warning_lines}\n  Stale ccx binary on PATH: $candidate; remove it or replace it with v$installed_version."
      fi
    fi
  done

  if [ -n "$synced_paths" ]; then
    printf '\nAlso updated duplicate installs:%b\n' "$synced_paths"
  fi
  if [ -n "$warning_lines" ]; then
    printf '\nWarning:%b\n' "$warning_lines"
  fi
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

ACTIVE_CCX="$(active_ccx_path)"

if [ "$ACTIVE_CCX" != "$INSTALL_DIR/ccx" ]; then
  ensure_path_block
fi

echo ""
echo "✓ CCX installed to $INSTALL_DIR/ccx"
if [ -n "$PROFILE_UPDATED" ]; then
  echo "✓ Updated shell profile: $PROFILE_UPDATED"
fi

reconcile_existing_installs

echo ""
if [ "$ACTIVE_CCX" != "$INSTALL_DIR/ccx" ]; then
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
echo "  # Claude subscription (auto-detected via OAuth):"
echo "  ccx"
