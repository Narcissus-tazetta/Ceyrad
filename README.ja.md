# Ceyrad

[English version is here](README.md)

Apple Musicで再生中の曲をDiscordのステータス（Rich Presence）に表示するmacOSメニューバーアプリ。

- 曲名・アーティスト・アルバムアート・再生プログレスバーを「〜を再生中」として表示
- ボタン最大2個（曲/アーティスト/アルバムページ、カスタムURL、リポジトリ）
- **Apple Musicが起動していないときは何もしない**（ポーリングなし・完全イベント駆動）

## 動作環境

- macOS 13以降
- Discordデスクトップアプリ

## インストール

1. [Releases](https://github.com/Narcissus-tazetta/Ceyrad/releases) から最新の `Ceyrad.app` をダウンロードし、`/Applications` に移動する
2. `Ceyrad.app` を開く
   - 野良アプリ（ad-hoc署名でApple公証なし）のため、初回起動時に「開発元を確認できません」と表示されることがある。その場合は以下のいずれかで開く
     - Finderでアプリを右クリック→「開く」
     - それでもブロックされる場合は「システム設定 > プライバシーとセキュリティ」を開き、下の方にある「"Ceyrad"は開発元を確認できないため使用がブロックされました」の表示から「このまま開く」を選択する
3. メニューバーに ♪ アイコンが表示されれば起動完了

## 使い方

1. Apple Musicで曲を再生する
2. 初回はmacOSが「Apple Musicの制御を許可しますか」と聞いてくるので **許可** する
   （再生位置の取得に使う。拒否してもアプリは動くが、プログレスバーだけ出なくなる）
3. Discordを起動していれば数秒でステータスに反映される。Discordを後から起動した場合も自動で検知して接続する

## メニューバーからの設定

| 項目 | 内容 |
|---|---|
| ボタン1 / ボタン2 | 遷移先: 曲ページ / アーティストページ / アルバムページ / カスタムURL / リポジトリ / 無効。「ラベルを変更…」でボタンの表示文言（32文字まで）も変えられる |
| Set Custom URL… | リンク先「カスタムURL」で使うURL |
| Set Repository URL… | リポジトリボタンと、URL解決失敗時のフォールバック先 |
| When Paused | 一時停止時の挙動: 表示継続 / 即消す / 1・3・5・10分後に消す（既定: 5分後） |
| Launch at Login | ログイン時にCeyradを自動起動する（クリックでオン/オフ切り替え） |
| Language | メニュー（この設定画面）の表示言語を English / 日本語 で切り替える。Discord側に表示される文言（接続ステータスやボタンラベルなど）は対象外で常に英語のまま |
| Reconnect to Discord | Discordへの接続をやり直す |
| Check for Updates… | [Sparkle](#自動更新sparkle)で最新版を確認・インストール |

## ログイン時に自動起動（任意）

メニューバーの「ログイン時に自動起動」をクリックしてオンにする。
（「システム設定 > 一般 > ログイン項目」に `Ceyrad.app` を手動で追加しても同じ）

## よくある質問

**再生中に本アプリを起動（再起動）しても曲が反映されない**
起動時の曲情報の取得にはオートメーション権限が必要です。「システム設定 > プライバシーとセキュリティ > オートメーション」で「ミュージック」がオンになっているか確認してください。オンにできない・一覧にない場合は一度アプリを再起動すると許可ダイアログが再表示されます。権限がない間も曲の切り替え・一時停止などの通知は届くため、次の操作からは反映されます（再生位置のプログレスバーは出ません）。メニューバーに「Music Control: Not Authorized ⚠️」と出ている場合はこの状態です。

**ボタンが自分のプロフィールに表示されない**
Discordの仕様で、RPCのボタンは自分自身からは見えません。別アカウントか他の人に確認してもらってください。

**アルバムアートが出ない曲がある**
アートワークとリンクはiTunes Search API（Apple Musicカタログ）で解決しています。ローカル取り込み曲やカタログに存在しない曲はアートなし・ボタンはリポジトリURLにフォールバックします。feat.表記や「- Single」などの表記揺れは吸収しますが、macOSの「言語と地域」の地域とApple Musicのストアフロント（国）が違う場合も、その国のカタログに曲がなく解決できないことがあります。

**Discordを後から起動した**
自動で検知して接続します（Musicが再生中なら数秒でステータスが出ます）。出ない場合はメニューの「Reconnect to Discord」。

---

## 開発

ここから先はソースからビルドする人・コントリビュートする人向け。

設計の詳細は [DESIGN.md](DESIGN.md) を参照。

### ビルド

Xcode Command Line Tools（Swift 5.9+）が必要。

```sh
./make-app.sh
mv Ceyrad.app /Applications/
```

`.app` にするとオートメーション権限がアプリ自体に紐づき（ターミナル起動時の権限問題が起きない）、システム設定の「ログイン項目」にも登録できる。

素の実行ファイルだけ欲しい場合は `swift build -c release`（`.build/release/Ceyrad`）。ログイン時の自動起動をLaunchAgentで行う場合は以下のようなplistを `~/Library/LaunchAgents/local.ceyrad.plist` に作成する。

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>local.ceyrad</string>
	<key>ProgramArguments</key>
	<array>
		<string>/path/to/.build/release/Ceyrad</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<false/>
</dict>
</plist>
```

```sh
launchctl load ~/Library/LaunchAgents/local.ceyrad.plist
```

### Discord Application ID

Discordに接続するためのApplication ID（`SettingsStore.discordClientId`）はこのアプリ専用のものを [Sources/Ceyrad/SettingsStore.swift](Sources/Ceyrad/SettingsStore.swift) に固定で埋め込んである。フォークして自分用に使う場合は、[Discord Developer Portal](https://discord.com/developers/applications) で自分のApplicationを作り、名前を **Apple Music** にした上で（この名前がステータスの「Apple Music を再生中」に使われる）そのApplication IDに書き換える。

### テスト

```sh
swift test
```

ActivityBuilderのペイロード生成、iTunes検索のマッチング、AppleScript出力のパースなど、純粋なロジックを`Tests/CeyradTests`でカバーしている。

### Lint

```sh
./scripts/lint.sh          # swift-format lint + swiftlint
./scripts/lint.sh --fix    # swift-format format -i + swiftlint --fix
```

[SwiftLint](https://github.com/realm/SwiftLint)（`.swiftlint.yml`）と[swift-format](https://github.com/swiftlang/swift-format)（`.swift-format`、Swiftツールチェーン同梱の`swift format`コマンドを使用）で構成。PR・pushのたびに [`ci.yml`](.github/workflows/ci.yml) がlint・テスト・ビルドを実行する。

### 自動更新（Sparkle）

[Sparkle](https://sparkle-project.org/)でアプリ内から更新を確認できる（メニューの「Check for Updates…」、または`SUEnableAutomaticChecks`により1日1回のバックグラウンドチェック）。

- 更新情報は [`appcast.xml`](appcast.xml) から取得する（`main`ブランチをraw.githubusercontent.com経由で参照）
- 配布物自体はEdDSA署名で検証する（コード署名はad-hocのため、こちらが実質的な改ざん検知の要）

#### メンテナ向け: 署名鍵のセットアップ（初回のみ）

新しいバージョンをリリースするたびに`appcast.xml`へ自動反映させるには、EdDSA署名鍵をGitHub Actionsのシークレットに登録する必要がある。

1. Sparkleのリリース（[Sparkle-*.tar.xz](https://github.com/sparkle-project/Sparkle/releases)、または`swift package resolve`後の`.build/artifacts/sparkle/Sparkle/bin/`）にある`generate_keys`を実行する

   ```sh
   ./generate_keys
   ```

   鍵はmacOSキーチェーンに保存され、公開鍵が表示される。**この公開鍵は既に`Support/Info.plist`の`SUPublicEDKey`に設定済み**（このセットアップを行った時点のもの）。もし鍵を作り直した場合はここを更新すること。

2. 秘密鍵をエクスポートする（ファイルの中身は絶対にコミットしない）

   ```sh
   ./generate_keys -x /tmp/sparkle_private_key.txt
   ```

3. GitHubリポジトリの **Settings > Secrets and variables > Actions** で `SPARKLE_PRIVATE_KEY` という名前のシークレットを作り、上記ファイルの中身を貼り付ける。登録が終わったら `/tmp/sparkle_private_key.txt` は削除する。

シークレット未設定の場合、リリースワークフローは通常通りzip/dmgを公開するだけで、`appcast.xml`の更新はスキップされる（アプリは起動するが自動更新は機能しない）。

### リリース

`v*.*.*` 形式のタグをpushすると、GitHub Actions（`.github/workflows/release.yml`）が自動でビルドし、`Ceyrad.app` をzipにしてGitHub Releaseへ添付する。

```sh
git tag v1.0.0
git push origin v1.0.0
```

配布されるzipはad-hoc署名（Apple Developer証明書での署名・notarizeなし）のため、ダウンロードして初回起動する際はGatekeeperに「開発元を確認できません」と表示される。その場合はFinderでアプリを右クリック→「開く」から起動するか、`xattr -cr Ceyrad.app` でquarantine属性を外す。

タグを打つと`Support/Info.plist`の`CFBundleShortVersionString`/`CFBundleVersion`がタグのバージョン（例: `v1.2.0` → `1.2.0`）に自動で書き換えられてからビルドされる。`SUPublicEDKey`が設定済みで`SPARKLE_PRIVATE_KEY`シークレットが登録されていれば、更新用zipの署名と[appcast.xml](#自動更新sparkle)への追記・pushも同じワークフローで行われる。
