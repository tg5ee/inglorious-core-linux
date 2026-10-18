#!/bin/bash
# Builds Inglorious Core Linux from this folder and installs it for the current user:
# ~/.local/bin/inglorious-core, an app-launcher entry, and (with sudo) the udev rule
# that lets your login session open the keyboard. Safe to run again after an update.
set -euo pipefail
cd "$(dirname "$0")"

command -v cargo >/dev/null || { echo "Rust is needed first: sudo pacman -S --needed rust" >&2; exit 1; }
# Build outside the plugin folder so ~/.config does not fill with build output.
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/inglorious-core/target"
cargo build --release --locked

install -Dm755 "$CARGO_TARGET_DIR/release/inglorious-core" "$HOME/.local/bin/inglorious-core"
install -Dm644 /dev/stdin "$HOME/.local/share/applications/inglorious-core.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Inglorious Core Linux
Comment=Lighting, actuation and keymap for the Glorious GMMK 3 HE 75% (unofficial)
Exec=$HOME/.local/bin/inglorious-core
Icon=input-keyboard
Categories=Settings;HardwareSettings;
StartupWMClass=inglorious-core
DESKTOP

# Leave any existing rule for this vendor alone; it may cover other devices too.
if ! grep -qs 'ATTRS{idVendor}=="342d".*uaccess' /etc/udev/rules.d/*.rules; then
  echo "Installing /etc/udev/rules.d/70-glorious.rules so your session can open the keyboard (needs sudo)."
  sudo install -Dm644 packaging/70-glorious.rules /etc/udev/rules.d/70-glorious.rules
  sudo udevadm control --reload
  sudo udevadm trigger --subsystem-match=hidraw
fi
echo "Installed. Open it from the bar icon or the app launcher."
