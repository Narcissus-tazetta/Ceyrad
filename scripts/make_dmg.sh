#!/usr/bin/env bash
# Cadence.app から配布用のDMGを作る。
# create-dmgがあれば見た目の整ったDMGを、なければhdiutilで最低限のDMGを作る。
set -euo pipefail

APP_NAME="Cadence"
VERSION="${1:-}"
CREATE_DMG_COMMAND="${CREATE_DMG_COMMAND:-create-dmg}"

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
APP_DIR="$ROOT_DIR/${APP_NAME}.app"
DMG_BACKGROUND_SOURCE="$ROOT_DIR/docs/background.tiff"

if [[ -z "$VERSION" ]] && [[ -f "$ROOT_DIR/Support/Info.plist" ]]; then
  VERSION="$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$ROOT_DIR/Support/Info.plist" 2>/dev/null || true)"
fi
VERSION="${VERSION:-dev}"

DMG_PATH="$DIST_DIR/${APP_NAME}-v${VERSION}-macos.dmg"

if [[ ! -d "$APP_DIR" ]]; then
  echo "${APP_NAME}.app が見つかりません。先に ./make-app.sh を実行してください: $APP_DIR" >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
rm -f "$DMG_PATH"

STAGE_DIR="$DIST_DIR/dmg-stage"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
cp -R "$APP_DIR" "$STAGE_DIR/"

DMG_OPTS=(
  --volname "$APP_NAME"
  --window-size 600 400
  --icon-size 100
  --icon "$APP_NAME.app" 150 200
  --app-drop-link 450 200
  --format UDZO
)

if [[ -f "$DMG_BACKGROUND_SOURCE" ]]; then
  mkdir -p "$STAGE_DIR/.background"
  sips -s format png "$DMG_BACKGROUND_SOURCE" --out "$STAGE_DIR/.background/background.png" >/dev/null
  DMG_OPTS+=(--background "$STAGE_DIR/.background/background.png")
fi

if command -v "$CREATE_DMG_COMMAND" >/dev/null 2>&1; then
  echo "[1/1] create-dmgでDMGを作成中..."
  "$CREATE_DMG_COMMAND" "${DMG_OPTS[@]}" "$DMG_PATH" "$STAGE_DIR"
else
  echo "create-dmgが見つかりません。hdiutilでフォールバック作成します。" >&2
  ln -s /Applications "$STAGE_DIR/Applications"
  hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE_DIR" -ov -format UDZO "$DMG_PATH"
fi

rm -rf "$STAGE_DIR"

echo "DMG: $DMG_PATH"
ls -lh "$DMG_PATH"
