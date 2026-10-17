#!/bin/sh
# Turns the icons of the design (lines, in icons/) into the symbolic icons
# GTK needs (filled shapes, in resources/icons/), so GTK can color them.
# Needs picosvg: pip install picosvg
set -eu
cd "$(dirname "$0")"
out=../resources/icons/scalable/actions
mkdir -p "$out"
for source in dc-*.svg; do
    picosvg "$source" > "$out/${source%.svg}-symbolic.svg"
done

# The list of files glib-compile-resources puts in the app (see build.rs).
{
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<gresources>'
    echo '  <gresource prefix="/io/github/geekabel/DeskCheck">'
    echo '    <file>style.css</file>'
    echo '    <file>style-dark.css</file>'
    for icon in "$out"/dc-*-symbolic.svg; do
        echo "    <file>icons/scalable/actions/$(basename "$icon")</file>"
    done
    echo '  </gresource>'
    echo '</gresources>'
} > ../resources/resources.gresource.xml
