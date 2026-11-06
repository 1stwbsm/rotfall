#!/bin/sh
# Regenerates the menu art. Takes around 20 minutes on a 12-thread CPU.
cd "$(dirname "$0")/.." || exit 1
FONT=${1:-/usr/share/fonts/TTF/liberation/LiberationSans-Bold.ttf}
nice blender -b -P tools/menu_scene.py -- logo "$PWD/assets/menu_logo.png" 1400 400 128 "$FONT" && \
nice blender -b -P tools/menu_scene.py -- scene "$PWD/assets/menu_bg.png" 1920 1080 128 && \
nice blender -b -P tools/menu_scene.py -- thumb "$PWD/assets/thumbnail_302x170.png" 302 170 256 "$FONT" && \
nice blender -b -P tools/menu_scene.py -- thumb "$PWD/assets/thumbnail_600x600.png" 600 600 192 "$FONT" && \
nice blender -b -P tools/menu_scene.py -- icon "$PWD/assets/icon_80x80.png" 80 80 256 "$FONT"
