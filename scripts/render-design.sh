#!/bin/sh
# Render Design/boards/*.html (or the given boards) to Design/rendered/boards/<board>.png
# with headless Chromium, so the design can be read without opening a browser.
set -e
cd "$(dirname "$0")/../Design/boards"
mkdir -p ../rendered/boards
BROWSER=$(command -v chromium || command -v google-chrome-stable || command -v google-chrome)
[ -n "$BROWSER" ] || { echo "Chromium or Chrome is required" >&2; exit 1; }
BOARDS=${*:-$(ls *.html)}
for board in $BOARDS; do
  out="../rendered/boards/${board%.html}.png"
  # Height is generous; the page background shows below the last state.
  "$BROWSER" --headless --disable-gpu --hide-scrollbars --allow-file-access-from-files \
    --force-device-scale-factor=1 --window-size=1300,${HEIGHT:-4200} \
    --screenshot="$out" "file://$PWD/$board" >/dev/null 2>&1
  echo "rendered $out"
done
