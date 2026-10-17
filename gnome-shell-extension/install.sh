#!/bin/sh
# Installs the DeskCheck extension for GNOME Shell, for this user.
# Only needed on Wayland: on X11, DeskCheck reads the mouse itself.
set -eu
cd "$(dirname "$0")"
uuid=deskcheck-pointer@geekabel.github.io
major=$(gnome-shell --version | sed 's/[^0-9]*\([0-9]*\).*/\1/')
if [ "$major" -ge 45 ]; then kind=esm; else kind=legacy; fi
target="$HOME/.local/share/gnome-shell/extensions/$uuid"
mkdir -p "$target"
cp "$kind/metadata.json" "$kind/extension.js" "$target/"
echo "Installed for GNOME Shell $major in $target"
echo "Log out and back in (Wayland), then run:"
echo "  gnome-extensions enable $uuid"
