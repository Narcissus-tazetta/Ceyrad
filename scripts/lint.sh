#!/usr/bin/env bash
# swiftlint と swift-format のチェックをまとめて実行する。
# --fix を付けると swift-format のフォーマットを適用する（swiftlintの自動修正はauto-correct参照）。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v swiftlint >/dev/null 2>&1; then
  echo "swiftlintが見つかりません。'brew install swiftlint' でインストールしてください。" >&2
  exit 1
fi

if [[ "${1:-}" == "--fix" ]]; then
  echo "[1/2] swift format format -i"
  swift format format -i -r --configuration .swift-format Sources
  echo "[2/2] swiftlint --fix"
  swiftlint --fix --quiet
else
  echo "[1/2] swift format lint"
  swift format lint -r --configuration .swift-format --strict Sources
  echo "[2/2] swiftlint lint"
  swiftlint lint --quiet --strict
fi
