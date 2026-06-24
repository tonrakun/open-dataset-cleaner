#!/usr/bin/env bash
# Install open-dataset-cleaner (odc) for Linux/macOS.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/tonrakun/open-dataset-cleaner/main/scripts/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- v0.1.0   # install a specific version
#
# Env overrides (mainly for testing / advanced use):
#   ODC_VERSION         tag to install (default: latest)
#   ODC_INSTALL_DIR      install directory (default: $HOME/.odc/bin)
#   ODC_ASSET_URL        full URL to the archive, bypasses version/target lookup
#   ODC_OS / ODC_ARCH     force OS/arch detection
#   ODC_NO_MODIFY_PATH    if set to 1, skip editing shell rc files
set -euo pipefail

REPO="tonrakun/open-dataset-cleaner"
BIN_NAME="odc"

VERSION="${ODC_VERSION:-${1:-latest}}"
INSTALL_DIR="${ODC_INSTALL_DIR:-$HOME/.odc/bin}"
NO_MODIFY_PATH="${ODC_NO_MODIFY_PATH:-0}"

for arg in "$@"; do
  case "$arg" in
    --no-modify-path) NO_MODIFY_PATH=1 ;;
  esac
done

log() { printf '[odc-install] %s\n' "$1"; }
die() { printf '[odc-install] error: %s\n' "$1" >&2; exit 1; }

detect_os() {
  if [ -n "${ODC_OS:-}" ]; then
    echo "$ODC_OS"
    return
  fi
  case "$(uname -s)" in
    Linux) echo linux ;;
    Darwin) echo darwin ;;
    *) die "unsupported OS: $(uname -s)" ;;
  esac
}

detect_arch() {
  if [ -n "${ODC_ARCH:-}" ]; then
    echo "$ODC_ARCH"
    return
  fi
  case "$(uname -m)" in
    x86_64|amd64) echo x86_64 ;;
    arm64|aarch64) echo aarch64 ;;
    *) die "unsupported arch: $(uname -m)" ;;
  esac
}

target_triple() {
  os="$1"; arch="$2"
  case "$os" in
    linux) echo "${arch}-unknown-linux-gnu" ;;
    darwin) echo "${arch}-apple-darwin" ;;
  esac
}

main() {
  command -v curl >/dev/null 2>&1 || die "curl is required"
  command -v tar >/dev/null 2>&1 || die "tar is required"

  os="$(detect_os)"
  arch="$(detect_arch)"
  target="$(target_triple "$os" "$arch")"

  if [ -n "${ODC_ASSET_URL:-}" ]; then
    asset_url="$ODC_ASSET_URL"
    resolved_version="(custom url)"
  else
    if [ "$VERSION" = "latest" ]; then
      log "resolving latest release..."
      resolved_version="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
      [ -n "$resolved_version" ] || die "could not resolve latest release tag"
    else
      resolved_version="$VERSION"
    fi
    asset_url="https://github.com/$REPO/releases/download/${resolved_version}/${BIN_NAME}-${target}.tar.gz"
  fi

  log "version: $resolved_version"
  log "target:  $target"
  log "url:     $asset_url"

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT

  archive="$tmp_dir/odc.tar.gz"
  log "downloading..."
  curl -fsSL "$asset_url" -o "$archive" || die "download failed: $asset_url"

  log "extracting..."
  tar xzf "$archive" -C "$tmp_dir"

  bin_path="$(find "$tmp_dir" -type f -name "$BIN_NAME" | head -n1)"
  [ -n "$bin_path" ] || die "binary '$BIN_NAME' not found in archive"

  mkdir -p "$INSTALL_DIR"
  install -m 755 "$bin_path" "$INSTALL_DIR/$BIN_NAME"
  log "installed to $INSTALL_DIR/$BIN_NAME"

  if [ "$NO_MODIFY_PATH" != "1" ]; then
    add_to_path
  else
    log "skipping PATH update (--no-modify-path)"
  fi

  log "done. run \"$BIN_NAME --help\" to get started (open a new shell, or 'source' your rc file, if PATH was just updated)."
}

add_to_path() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*)
      log "PATH already contains $INSTALL_DIR"
      return
      ;;
  esac

  rc_file=""
  case "${SHELL:-}" in
    */zsh) rc_file="$HOME/.zshrc" ;;
    */bash) rc_file="$HOME/.bashrc" ;;
    *) rc_file="$HOME/.profile" ;;
  esac

  line="export PATH=\"$INSTALL_DIR:\$PATH\""
  if [ -f "$rc_file" ] && grep -qF "$INSTALL_DIR" "$rc_file" 2>/dev/null; then
    log "$rc_file already references $INSTALL_DIR"
    return
  fi

  printf '\n# added by open-dataset-cleaner installer\n%s\n' "$line" >> "$rc_file"
  log "added $INSTALL_DIR to PATH via $rc_file"
}

main "$@"
