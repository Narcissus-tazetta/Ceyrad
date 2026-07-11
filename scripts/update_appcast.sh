#!/usr/bin/env bash
# リリース済みのzipをSparkleの秘密鍵で署名し、appcast.xmlに新しいitemを追記する。
# リリースワークフロー（.github/workflows/release.yml）から呼ばれる想定。
#
# 必須の環境変数:
#   SPARKLE_PRIVATE_KEY  - `generate_keys -x` でエクスポートしたEdDSA秘密鍵の中身
#
# 使い方: scripts/update_appcast.sh <zip-path> <version e.g. 1.0.1> <download-url>
set -euo pipefail

ZIP_PATH="$1"
VERSION="$2"
DOWNLOAD_URL="$3"

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APPCAST="$ROOT_DIR/appcast.xml"

if [[ -z "${SPARKLE_PRIVATE_KEY:-}" ]]; then
  echo "SPARKLE_PRIVATE_KEYが設定されていません。appcast.xmlの更新をスキップします。" >&2
  exit 1
fi

SPARKLE_BIN="$(dirname "$(find "$ROOT_DIR/.build/artifacts" -maxdepth 5 -iname generate_keys -print -quit)")"
if [[ -z "$SPARKLE_BIN" || ! -x "$SPARKLE_BIN/sign_update" ]]; then
  echo "Sparkleのbin/ツールが見つかりません。先に 'swift package resolve' を実行してください。" >&2
  exit 1
fi

# sign_updateは `sparkle:edSignature="..." length="..."` の形式で出力する
SIGN_OUTPUT="$(echo "$SPARKLE_PRIVATE_KEY" | "$SPARKLE_BIN/sign_update" --ed-key-file - "$ZIP_PATH")"
ED_SIGNATURE="$(echo "$SIGN_OUTPUT" | sed -n 's/.*sparkle:edSignature="\([^"]*\)".*/\1/p')"
LENGTH="$(echo "$SIGN_OUTPUT" | sed -n 's/.*length="\([^"]*\)".*/\1/p')"

if [[ -z "$ED_SIGNATURE" || -z "$LENGTH" ]]; then
  echo "sign_updateの出力を解析できませんでした: $SIGN_OUTPUT" >&2
  exit 1
fi

PUB_DATE="$(date -u +"%a, %d %b %Y %H:%M:%S +0000")"

MIN_OS="$(python3 -c "
import re
text = open('$ROOT_DIR/Package.swift').read()
m = re.search(r'\.macOS\(\.v(\d+)\)', text)
print(m.group(1) + '.0' if m else '13.0')
")"

python3 "$ROOT_DIR/scripts/insert_appcast_item.py" \
  "$APPCAST" \
  --version "$VERSION" \
  --download-url "$DOWNLOAD_URL" \
  --pub-date "$PUB_DATE" \
  --ed-signature "$ED_SIGNATURE" \
  --length "$LENGTH" \
  --min-os "$MIN_OS"

echo "appcast.xml に v$VERSION を追加しました。"
