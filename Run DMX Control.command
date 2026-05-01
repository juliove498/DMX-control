#!/usr/bin/env bash
# Launches DMX Control with the privileges libusb needs to claim the FTDI
# dongle. macOS prompts for your password once per Terminal session.
#
# Usage: double-click this file. Terminal opens, asks for your password,
# and the app launches with USB access. Closing Terminal also closes the
# app — that's normal.
#
# The `sudo -E HOME=...` part is critical: by default sudo flips $HOME to
# /var/root, which would make the app save its config in root's home dir
# and silently lose anything you configured. Forwarding HOME means the
# autosave / show / log files all land in *your* ~/Library/... like normal.

set -e
APP="/Applications/DMX Control.app/Contents/MacOS/dmx-control"
if [ ! -x "$APP" ]; then
    echo "DMX Control no está en /Applications. Movelo ahí primero."
    read -r -p "Press Enter to close..."
    exit 1
fi
USER_HOME="$HOME"
exec sudo -E HOME="$USER_HOME" "$APP"
