#!/usr/bin/env bash
# Builds dist/cosmic-advanced-player-<version>-<arch>.AppImage from an existing release build.
# Usage: packaging/appimage.sh [arch]   (x86_64 or aarch64, default: uname -m)
set -euo pipefail
cd "$(dirname "$0")/.."
ARCH="${1:-$(uname -m)}"
APPID=io.github.kisakay.CosmicAdvancedPlayer
NAME=cosmic-advanced-player
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
WORK=$(mktemp -d)
APPDIR="$WORK/AppDir"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/metainfo" \
  "$APPDIR/usr/share/icons/hicolor/scalable/apps" dist
install -m755 "target/release/$NAME" "$APPDIR/usr/bin/$NAME"
install -m644 "resources/$APPID.desktop" "$APPDIR/usr/share/applications/"
install -m644 "resources/$APPID.metainfo.xml" "$APPDIR/usr/share/metainfo/$APPID.appdata.xml"
install -m644 "resources/icons/hicolor/scalable/apps/$APPID.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/"
cp "resources/$APPID.desktop" "$APPDIR/"
cp "resources/icons/hicolor/scalable/apps/$APPID.svg" "$APPDIR/$APPID.svg"
ln -sf "$APPID.svg" "$APPDIR/.DirIcon"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export XDG_DATA_DIRS="$HERE/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "$HERE/usr/bin/cosmic-advanced-player" "$@"
RUN
chmod +x "$APPDIR/AppRun"

TOOL="$WORK/appimagetool.AppImage"
curl -fsSL -o "$TOOL" \
  "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
chmod +x "$TOOL"
# No FUSE on CI runners: let the tool extract itself.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" --no-appstream "$APPDIR" "dist/$NAME-$VERSION-$ARCH.AppImage"
echo "dist/$NAME-$VERSION-$ARCH.AppImage"
