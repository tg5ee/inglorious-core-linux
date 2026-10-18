#!/bin/bash
# Removes what install.sh added. Your saved settings in ~/.config/glorious-core-rs stay,
# and so does everything stored on the keyboard itself.
set -euo pipefail
rm -f "$HOME/.local/bin/inglorious-core" \
  "$HOME/.local/share/applications/inglorious-core.desktop"
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/inglorious-core"
# Only remove the rule if it is still exactly the one install.sh put there.
if cmp -s "$(dirname "$0")/packaging/70-glorious.rules" /etc/udev/rules.d/70-glorious.rules; then
  echo "Removing /etc/udev/rules.d/70-glorious.rules (needs sudo)."
  sudo rm /etc/udev/rules.d/70-glorious.rules
  sudo udevadm control --reload
fi
echo "Removed. Run 'omarchy plugin remove io.github.tg5ee.inglorious-core-linux' to drop the bar icon."
