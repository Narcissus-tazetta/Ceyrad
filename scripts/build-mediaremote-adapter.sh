#!/bin/sh
# YouTube Music の検知に使う mediaremote-adapter（BSD-3-Clause）を取得してビルドする。
# 成果物は .build/mediaremote-adapter/build/ に出る。make-app.sh が Ceyrad.app へ同梱する。
#
# タグではなくコミットで固定する: タグは付け替えられるが、配布物に入るコードは
# ビルドのたびに同じでなければならない。
set -e
cd "$(dirname "$0")/.."

COMMIT=e3ff5021eb0875858bd05f48d2e9ba2e962d1cf6 # v0.7.7
DIR=.build/mediaremote-adapter

command -v cmake >/dev/null 2>&1 || {
    echo "cmake が見つかりません。'brew install cmake' でインストールしてください。" >&2
    exit 1
}

if [ "$(git -C "$DIR" rev-parse HEAD 2>/dev/null)" != "$COMMIT" ]; then
    rm -rf "$DIR"
    git init -q "$DIR"
    git -C "$DIR" fetch -q --depth 1 https://github.com/ungive/mediaremote-adapter.git "$COMMIT"
    git -C "$DIR" checkout -q FETCH_HEAD
fi

cmake -S "$DIR" -B "$DIR/build" -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build "$DIR/build" >/dev/null
