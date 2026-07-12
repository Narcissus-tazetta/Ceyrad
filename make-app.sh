#!/bin/sh
# SPMのreleaseバイナリから Ceyrad.app を組み立てる。
# .appにすると、オートメーション権限（TCC）が起動元のターミナルではなく
# このアプリ自体に紐づき、ログイン項目にもそのまま登録できる。
set -e
cd "$(dirname "$0")"

swift build -c release

APP="Ceyrad.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp .build/release/Ceyrad "$APP/Contents/MacOS/Ceyrad"
cp Support/Info.plist "$APP/Contents/Info.plist"
cp Support/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# SparkleはSPMがバイナリターゲットとして .build 配下にコピーする。
# 通常の.appバンドル同様 Contents/Frameworks に入れ、実行ファイルの
# rpathにそこを追加する（SPMが付与する@loader_pathはMacOSディレクトリ基準のため別途必要）。
cp -R .build/release/Sparkle.framework "$APP/Contents/Frameworks/Sparkle.framework"
install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP/Contents/MacOS/Ceyrad"

# ad-hoc署名。Apple IDの開発証明書があればそれを使うと、
# 再ビルドしてもTCCの許可が維持されやすい。
# Sparkle.frameworkの配布物は既に有効なad-hoc署名を持っているためコピーのままでよく、
# 外側のアプリだけ署名すればいい（Xcodeの「Sign on Copy」と同じ考え方）。
codesign --force --sign - "$APP"

echo "Created $APP"
echo "インストール: mv $APP /Applications/"

