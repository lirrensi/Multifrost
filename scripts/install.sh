#!/bin/sh
set -eu

# Multifrost Router — one-line installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lirrensi/Multifrost/main/scripts/install.sh | sh
#   curl -fsSL ... | sh -s -- --version v5.0.0
#   sh install.sh --help

GITHUB_REPO="${MULTIFROST_INSTALL_REPO:-lirrensi/Multifrost}"
INSTALL_DIR="${MULTIFROST_INSTALL_DIR:-}"
BIN_BASE="multifrost-router"
VERSION=""

# ── helpers ──────────────────────────────────────────────────────────────────

usage() {
  cat <<EOF
Usage: $0 [--version vX.Y.Z] [--dir /path] [--help]

One-line installer for the Multifrost router binary.

Options:
  --version vX.Y.Z   Install a specific version (default: latest release)
  --dir /path        Install directory (default: ~/.local/bin on Linux/macOS)

The script detects your OS and architecture automatically.
EOF
  exit 0
}

err() {
  printf "Error: %s\n" "$1" >&2
  exit 1
}

# ── parse args ───────────────────────────────────────────────────────────────

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      shift
      [ $# -eq 0 ] && err "--version requires a value (e.g. --version v5.0.0)"
      VERSION="$1"
      ;;
    --dir)
      shift
      [ $# -eq 0 ] && err "--dir requires a path"
      INSTALL_DIR="$1"
      ;;
    --help|-h) usage ;;
    *) err "unknown flag: $1 (try --help)" ;;
  esac
  shift
done

# ── detect platform ──────────────────────────────────────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  PLATFORM_OS="linux" ;;
  Darwin) PLATFORM_OS="macos" ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    PLATFORM_OS="windows"
    ;;
  *) err "unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64|amd64) PLATFORM_ARCH="x86_64" ;;
  aarch64|arm64) PLATFORM_ARCH="aarch64" ;;
  *) err "unsupported architecture: $ARCH" ;;
esac

PLATFORM="${PLATFORM_OS}-${PLATFORM_ARCH}"

# ── determine version ────────────────────────────────────────────────────────

if [ -z "$VERSION" ]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" 2>/dev/null \
    | grep '"tag_name":' | sed 's/.*"tag_name":"//' | sed 's/".*//')"
  [ -z "$VERSION" ] && err "could not determine latest version from GitHub. Pass --version manually."
fi

# ── download URL ─────────────────────────────────────────────────────────────

case "$PLATFORM" in
  linux-x86_64)     ASSET="${BIN_BASE}-${PLATFORM}" ;;
  linux-aarch64)    ASSET="${BIN_BASE}-${PLATFORM}" ;;
  macos-x86_64)     ASSET="${BIN_BASE}-${PLATFORM}" ;;
  macos-aarch64)    ASSET="${BIN_BASE}-${PLATFORM}" ;;
  windows-x86_64)   ASSET="${BIN_BASE}-${PLATFORM}.exe" ;;
  *) err "no binary for platform: $PLATFORM" ;;
esac

DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/${VERSION}/${ASSET}"

# ── install directory ────────────────────────────────────────────────────────

if [ -z "$INSTALL_DIR" ]; then
  INSTALL_DIR="${HOME}/.local/bin"
fi

mkdir -p "$INSTALL_DIR"

if [ "$PLATFORM_OS" = "windows" ]; then
  BIN_NAME="${BIN_BASE}.exe"
else
  BIN_NAME="${BIN_BASE}"
fi
DEST="${INSTALL_DIR}/${BIN_NAME}"

# ── download and install ─────────────────────────────────────────────────────

printf "Installing Multifrost Router %s for %s...\n" "$VERSION" "$PLATFORM"
printf "  Downloading: %s\n" "$DOWNLOAD_URL"

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

HTTP_CODE="$(curl -fsSL -w '%{http_code}' -o "$TMP" "$DOWNLOAD_URL")"
if [ "$HTTP_CODE" != "200" ]; then
  err "download failed (HTTP $HTTP_CODE). Is version '$VERSION' correct?"
fi

cp "$TMP" "$DEST"
chmod +x "$DEST"

printf "  Installed: %s\n" "$DEST"

# ── PATH check ───────────────────────────────────────────────────────────────

case ":$PATH:" in
  *:"$INSTALL_DIR":*) ;;
  *)
    printf "\nNote: %s is not on your PATH.\n" "$INSTALL_DIR"
    printf "Add this to your shell config to fix it:\n"
    printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
    ;;
esac

printf "\nMultifrost Router installed. Run:\n  %s\n" "$BIN_NAME"
