#!/usr/bin/env bash
# Runs `npm run tauri dev` with the privileges libusb needs to claim the
# FTDI dongle, *without* the two pitfalls of plain `sudo npm run tauri dev`:
#
#   1. sudo flips $HOME to /var/root by default. The app saves its config
#      under that home, so the dev session sees nothing the release wrote
#      and vice-versa. We forward HOME so dev and release share the same
#      ~/Library/Application Support/dmx-control directory.
#
#   2. cargo writes its build artefacts to `src-tauri/target/` as root.
#      Next time you try a non-sudo `npm run tauri build` it fails with
#      EACCES. We point CARGO_TARGET_DIR at a separate dir so the normal
#      target/ stays user-owned.
#
# Usage: double-click. Terminal opens, asks your password once, dev server
# starts. Closing Terminal closes the dev session.

set -e
cd "$(dirname "$0")"

if ! command -v npm >/dev/null 2>&1; then
    echo "npm no está en el PATH. Instalá Node.js o agregá npm al PATH."
    read -r -p "Press Enter to close..."
    exit 1
fi

USER_HOME="$HOME"
USER_PATH="$PATH"
ROOT_TARGET="$USER_HOME/.cache/dmx-control-target-sudo"
mkdir -p "$ROOT_TARGET"

echo ">>> Dev with USB"
echo "    HOME              = $USER_HOME (forwarded)"
echo "    CARGO_TARGET_DIR  = $ROOT_TARGET (root-owned, isolated from main target/)"
echo

exec sudo \
    HOME="$USER_HOME" \
    PATH="$USER_PATH" \
    CARGO_TARGET_DIR="$ROOT_TARGET" \
    npm run tauri dev
