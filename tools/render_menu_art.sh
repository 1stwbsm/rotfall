#!/bin/sh
# Regenerates the menu art. Takes around 15 minutes on a 12-thread CPU.
cd "$(dirname "$0")/.." || exit 1
FONT=${1:-/usr/share/fonts/TTF/liberation/LiberationSans-Bold.ttf}
nice blender -b -P tools/menu_scene.py -- logo "$PWD/assets/menu_logo.png" 1400 400 128 "$FONT" && \
nice blender -b -P tools/menu_scene.py -- scene "$PWD/assets/menu_bg.png" 1920 1080 128
