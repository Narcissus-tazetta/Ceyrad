# Apple Music → Discord Rich Presence 設計書 v2

## 1. 概要

Apple Musicの再生中トラック情報をDiscordのステータス（Rich Presence）に表示するmacOSネイティブアプリ。
軽量・低リソース・Apple Music起動時のみ動作することを最優先とする。

v1からの主な変更点:

- Discord IPCソケットのパスを修正（`$TMPDIR/discord-ipc-{0..9}`）
- Catalog ID解決をMusicKit APIから **iTunes Search API** に変更（認証不要・無料）
- Discord側のライフサイクル（未起動・後起動・クラッシュ）に対する再接続ロジックを追加
- Activityタイプ `2`（Listening）を採用し「〜を再生中」表示にする
- `SET_ACTIVITY` のデバウンス、iTunes APIのレート制限・キャッシュを明記
- 一時停止時の挙動を設定化（デフォルト: ステータスを消す）
- `playlist` linkTypeを初期スコープから除外（後述）
- TCC（Automation権限）の扱いを明記

---

## 2. 要件

### 機能要件

- Discordステータスに以下を表示
  - アルバムアート（大アイコン、iTunes Search APIの512x512アートワークURL）
  - 曲名・アーティスト名・アルバム名（ホバーテキスト）
  - 再生プログレスバー（開始/終了タイムスタンプ）
- ボタンを0〜2個表示（Discord RPC仕様上、最大2個）
- ボタンの遷移先はユーザーが設定で選べる
  - 曲ページ / アーティストページ / アルバムページ / カスタムURL / リポジトリ / 無効
  - Catalog URLが解決できない曲（ローカル取り込み等）はそのボタンを表示しない
- Apple Musicが**起動していないときは一切動作しない**（監視・接続・通知購読すべて停止）
  - 例外はYouTube Musicの検知（§11）。利用者がオンにしたときだけ動く

### 非機能要件

- アイドル時CPU 0%近傍、メモリ数MB〜十数MB
- ポーリング禁止。イベント駆動のみ
- macOS 26.1以降を対象（26.0系のAppleScript不具合は対象外）

---

## 3. 全体アーキテクチャ

```
┌──────────────────────────────────────────────────────┐
│                MenuBarApp (Swift/AppKit, SPM)          │
│                                                        │
│  ┌────────────────────┐        ┌──────────────────┐   │
│  │ AppLifecycleWatcher │        │ SettingsStore    │   │
│  │ (NSWorkspace)       │        │ (UserDefaults)   │   │
│  │  - Music起動/終了    │        └──────────────────┘   │
│  │  - Discord起動 ★    │                 ▲             │
│  └─────────┬──────────┘                 │             │
│            ▼                            │             │
│  ┌────────────────────┐        ┌──────────────────┐   │
│  │ NowPlayingObserver  │───────▶│ ActivityBuilder  │   │
│  │ (playerInfo通知)     │        │ (type:2/ボタン/  │   │
│  └─────────┬──────────┘        │  タイムスタンプ)   │   │
│            │ 位置のみ補完         └────────┬─────────┘   │
│  ┌─────────▼──────────┐                 │             │
│  │ MusicAppleScript    │                 ▼             │
│  │ (通知発火時のみ)      │        ┌──────────────────┐   │
│  └────────────────────┘        │ DiscordRPCClient │   │
│  ┌────────────────────┐        │ ($TMPDIR/        │   │
│  │ ITunesSearchClient ★│───────▶│  discord-ipc-N)  │   │
│  │ (URL/アート解決)      │        └──────────────────┘   │
│  └────────────────────┘                                │
└──────────────────────────────────────────────────────┘
★ = v2で追加
```

---

## 4. コンポーネント詳細

### 4.1 AppLifecycleWatcher

- `NSWorkspace.shared.notificationCenter` で以下を購読
  - `didLaunchApplicationNotification` / `didTerminateApplicationNotification`
  - `com.apple.Music` → 全コンポーネントの起動/停止トリガー
  - **`com.hnc.Discord`（PTB/Canary含む）の起動も監視** → Musicが動いていれば即RPC接続
- 自アプリ起動時に `runningApplications` をチェックし、Music起動済みなら即開始
- これ以外のタイミングでは他コンポーネントを一切動かさない

### 4.2 NowPlayingObserver

- `DistributedNotificationCenter` の `com.apple.Music.playerInfo` を購読（完全イベント駆動）
- userInfoから `Name` / `Artist` / `Album` / `Player State` / `Total Time`(ms) を取得
- **通知には再生位置が含まれない** → 再生中の通知発火時のみ `MusicAppleScript.playerPosition()` で補完
- 自アプリ起動時にMusicが既に再生中の場合は通知が来ないため、起動2秒後に1回だけAppleScriptで全状態を取得する
- Music終了時は購読解除

### 4.3 MusicAppleScript

- AppleScript呼び出しは「通知発火時の位置補完」と「初期状態取得」の2箇所のみ。定期実行なし
- Apple Musicの取得にはMediaRemote私的フレームワークを使わない。macOS 15.4以降はエンタイトルメント制限があり、通知とAppleScriptで足りる。MediaRemoteを使うのはYouTube Musicの検知だけ（§11）
- 実数→文字列のロケール差（小数点カンマ）を吸収してパースする

### 4.4 ITunesSearchClient（v1のMusicKit案を置き換え）

- `https://itunes.apple.com/search`（認証不要・無料。MusicKitと違いDeveloper Program加入不要）
- 1リクエストで `trackViewUrl` / `artistViewUrl` / `collectionViewUrl` / `artworkUrl100` を取得
- アートワークは `100x100bb` → `512x512bb` に置換して高解像度化
- **マッチング**: 正規化（大文字小文字・全半角・ダイアクリティカル・全角スペース無視）した上で
  「曲+アーティスト+アルバム一致 → 曲+アーティスト一致 → feat.除去した曲+アーティスト一致 →
  曲のみ一致 → feat.除去した曲のみ一致」の順で採用。
  feat.表記（`(feat. X)` / `ft. X` など）とアルバムの「- Single / - EP」サフィックスは
  除去した形でも比較し、ローカルとカタログの表記揺れを吸収する。
  それ以外は不一致扱い（誤マッチで無関係な曲のリンクを出さない）
- **レート制限**: 最低3秒間隔（非公式に約20req/分の制限があるため）
- **キャッシュ**: トラック単位でLRU 300件。**見つからなかった結果もキャッシュ**する
  （ローカル曲の再生/一時停止のたびに再検索しない）。ネットワークエラーはキャッシュしない
- **結果は3値**（`CatalogOutcome`）: `found` / `missing`（カタログに無い＝確定した答え） /
  `failed`（訊けなかった＝答えではない）。`failed`だけは15秒後に訊き直す。
  2つを`nil`ひとつに潰すと、一時的な回線断がその曲のアートワークを永久に失わせる
- 国コードはシステムロケールから取得（`Locale.current.region`、fallback `US`）

### 4.5 ActivityBuilder

- **`type: 2`（Listening）** を指定 →「Apple Music を再生中」表示になる
- `details` = 曲名、`state` = アーティスト、`assets.large_text` = アルバム
  （Discord仕様: 各2〜128文字。短い場合はパディング）
- タイムスタンプ: `start = now - position`、`end = start + duration`（ms）。再生中のみ付与
- ボタン: 最大2個・label≤32文字・url≤512文字・http(s)のみをバリデーション。
  URL重複時は2個目を捨てる。Catalog解決失敗時はそのボタンを出さない
- 一時停止時（表示継続設定の場合）: タイムスタンプを外し曲名に「⏸ 」を付ける

### 4.6 DiscordRPCClient

- **ソケットパスは `$TMPDIR/discord-ipc-{0..9}`**（v1の記述は誤り）。0〜9を順に試す
- フレーム形式: `[op: UInt32 LE][length: UInt32 LE][JSON payload]`
- Handshake(op:0, client_id) → READY受信で接続確立 → `SET_ACTIVITY`(op:1)
- PING(op:3)にはPONG(op:4)で応答。CLOSE(op:2)・EOF・write失敗で切断処理
- `SO_NOSIGPIPE` を設定（切断済みソケットへのwriteでプロセスが落ちないように）
- Apple Music終了時: `SET_ACTIVITY null` → 切断（常時接続しない）

### 4.7 再接続戦略（v2で追加）

- 接続失敗/切断時: **指数バックオフ**（1s→2s→4s→…上限60s）で再試行
- Discordの起動をNSWorkspaceで検知したら、バックオフを待たず即接続
- バックオフはApple Music稼働中のみ動作し、Music終了で完全停止（軽量性の維持）
- メニューに手動「Discordに再接続」も用意

### 4.8 更新のデバウンス（v2で追加）

- RPCの `SET_ACTIVITY` はレート制限（約20秒に5回）があるため、送信を**0.8秒デバウンス**
- 曲送り連打時は最後の状態だけが送られる
- タイムスタンプは絶対時刻で計算するため、送信遅延で進捗バーはずれない

### 4.9 SettingsStore / MenuBarUI

- `UserDefaults` に永続化。設定項目:
  - `button1Type` / `button1Label` / `button2Type` / `button2Label`
  - `customURL` / `repositoryURL`
  - `clearOnPause`（デフォルト `true` = 一時停止でステータスを消す）
  - `discordClientId`
- UIはNSStatusItem + メニューのみ。メニューは開くたびに構築（常駐メモリ最小）
- 設定入力はNSAlert + テキストフィールド（設定ウィンドウを常駐させない）
- `LSUIElement = true`（リンカでInfo.plistを実行ファイルに埋め込み）

---

## 5. 確定した決定事項（v1の「追加で決めておくべきこと」への回答）

| 項目 | 決定 |
|---|---|
| Activityタイプ | `type: 2`（Listening）。古いDiscordが無視してもPlaying表示になるだけで無害 |
| デバウンス | 0.8秒。RPCレート制限対策 |
| TCC権限 | `NSAppleEventsUsageDescription` を埋め込みInfo.plistで宣言。初回のAppleScript実行時にmacOSが許可を求める。App Store配布はしない前提（直接配布 / Homebrew Cask） |
| ボタンの自己非表示 | RPCボタンは**自分のプロフィールでは見えない**（他人には見える）。仕様なのでREADMEに明記 |
| `playlist` linkType | 除外。ローカルの再生プレイリストをCatalogプレイリストに対応付ける確実な方法がないため |
| 一時停止時 | デフォルトでステータスをクリア。メニューから「表示継続（⏸付き・バーなし）」に切替可能 |
| Catalog解決 | iTunes Search API。失敗もキャッシュし、解決できないボタンは表示しない |
| Client ID | ハードコードせずUserDefaultsに保存。メニューから設定（未設定時はメニューに⚠️表示） |

---

## 6. 軽量化のための設計方針

| 施策 | 効果 |
|---|---|
| Apple Music起動時のみ全コンポーネント稼働 | 未起動時はCPU/メモリ消費ほぼゼロ |
| `DistributedNotificationCenter` 購読（ポーリングなし） | アイドル時CPU 0% |
| AppleScriptは通知発火時+初期化時のみ | プロセス間通信コスト最小 |
| Discord IPC接続はMusic稼働中のみ。再接続バックオフもMusic終了で停止 | 未使用時にソケット・タイマーを持たない |
| iTunes検索結果のLRUキャッシュ（ネガティブ含む） | 同じ曲の再検索・API負荷を排除 |
| メニューは開くたび構築、設定ウィンドウなし、`LSUIElement` | メモリフットプリント最小 |

---

## 7. 既知の制約

1. **アートワークはCatalogにある曲のみ**。ローカル取り込み曲はアートなし（Discordアプリ側のデフォルトアイコン表示）
2. **ボタンは自分では見えない**（Discord仕様）。確認は別アカウントか他人のプロフィールで
3. iTunes Search APIのマッチングは一致ベースのため、表記揺れが大きい曲は解決できないことがある（feat.表記差・「- Single」サフィックスは正規化で吸収済み）→ 解決できない曲はボタン非表示
4. ステータスに出す名称（"Apple Music" など）はDiscord Developer Portalで作るApplicationの名前で決まる

---

## 8. 複数ソース

音楽ソースは`MusicSource`（`appleMusic` / `youtubeMusic`）の2つ。状態は`MusicState`をソースごとに1つ持ち、
Discordに出すソースは`SourceSelector.select`が状態だけから決める純関数。

- 表示できる（`isDisplayable`）ソースが1つならそれ
- 両方なら再生中が一時停止に優先し、同じ状態どうしなら`playingSince`（`.playing`に遷移した時刻）が新しい方
- 時刻も並ぶ（どちらも再生したことがない）ときはApple Music

Discordに出る名前はApplication（client ID）で決まり、接続はApplicationごとなので、表示するソースが
変わったら`AppDelegate+Discord`が接続を閉じて新しいソースのclient IDで繋ぎ直す。繋ぎ直しの最中は、
別のApplicationの名前でActivityを送らないよう、`flushActivity`が接続中のソースと表示ソースの一致を確かめる。
接続しておくのは、表示中のソースがあればそれ、なければ起動中のApple Music。どちらもなければ繋がない。

## 9. macOS版とWindows版の一致（v6）

Discordへ送るActivityの組み立ては、Swift（`ActivityBuilder`）とRust（`core::activity_builder`）に1本ずつ実装がある。片方だけ直しても両方のテストが緑のままなので、乖離がリリースまで届く経路になっていた。

`spec/activity_vectors.json` に入力と期待値を1つだけ置き、`Tests/CeyradTests/SharedVectorTests.swift` と `windows/tests/shared_vector_tests.rs` の両方がそれを読む。実装は2本のままでも、仕様の乖離だけはCIで落ちる。

これで実際に見つかった不一致が1件ある。文字列の切り詰め単位が、Swiftは書記素クラスタ（`String.prefix`）、Rustはスカラー（`core::text::truncate`）で、家族絵文字だと同じ「32文字」が7倍ずれていた。Discordの上限はスカラー寄りなので、Swift側に`Text.truncate`を追加してRustの実装を移植し、両者を揃えてある。

---

## 10. 現在の構成（v6）

両OSで対になる型を揃えてある。片方に手を入れたらもう片方も見る。

| 役割 | macOS (Swift) | Windows (Rust) |
|---|---|---|
| プレイヤー固有の定数 | `AppleMusic` | `core::apple_music` |
| 音楽側の全状態 | `MusicState` | `core::models::MusicState` |
| Activity組み立て | `ActivityBuilder` | `core::activity_builder` |
| 文字列の切り詰め | `Text` | `core::text` |
| メニューの中身（純粋） | `MenuModel` | `core::menu_model` |
| メニューの描画 | `MenuBarController` | `tray::render` |
| ステータス行 | `StatusLinesBuilder` | `core::status_lines` |
| 設定 | `SettingsStore` | `core::settings_model` + `app::settings_store` |
| URL入力ループ | `URLPrompt` | `core::url_prompt` |
| 送信済み内容の記憶 | `AppDelegate.LastSent` | `core::presence::Presence` |
| 再接続バックオフ | `ReconnectBackoff` | `core::backoff::Backoff` |
| 表示ソースの選択 | `SourceSelector` | `core::source` |
| YouTube Musicの判定 | `YouTubeMusic` | `core::youtube_music` |
| YouTube Musicの検索 | `YouTubeMusicSearch` | `core::ytmusic` |
| Apple Musicの広告判定 | `TrackInfo.isAdvertisement` | `TrackInfo::is_advertisement` |
| 一時停止タイマー | `PauseHideTimer` | `core::timers`（`Timer::PauseHide`） |

**メニューはデータとして組み立てる。** `MenuModel` / `core::menu_model` はOS APIを一切呼ばず、
「いま何が出るか」を開かずにユニットテストで答えられる。AppKit・Win32への変換は薄い描画層が受け持つ。

**表示言語は引数で渡す。** `t(language, en, ja)` の形にしてあり、グローバルのシングルトンを読まない。
おかげでメニュー全体を両言語でテストに固定できる。Discordを名指しする文言（接続状態・"Reconnect to
Discord"・ボタンラベル）は翻訳対象外で、常に英語。

**同じ内容は再送しない。** Discordが同じに描画するActivityの再送はレート制限（20秒に5回）を
無駄に食うだけなので、直近の送信内容と比較して抑止する（`ActivityBuilder.isEquivalent` /
`activity_builder::is_equivalent`。タイムスタンプは2秒のドリフトを許容）。
接続が切れた/繋がった瞬間には**必ず忘れる**——新しい接続には何も表示されていないので、
覚えたままだと1発目が「前と同じ」と誤判定されて何も出なくなる。

---

## 11. YouTube Musicの検知（オプトイン、既定オフ）

ブラウザで再生中のYouTube Musicを、拡張機能なしで「YouTube Musicを再生中」として出す。メニューの
「Detect YouTube Music (browser)」をオンにしたときだけ動く。オフの間の常駐コストは増えない。

### 取得（macOS）

システム全体の「いま再生中」はMediaRemoteが持つが、macOS 15.4以降は`/usr/bin/perl`だけがそれを読む
エンタイトルメントを持つ。そこで[mediaremote-adapter](https://github.com/ungive/mediaremote-adapter)
（BSD-3-Clause）を`.app`に同梱し、perlに読み込ませて`stream`の標準出力をJSONで受け取る（`NowPlayingAdapter`）。

- コミットで固定してビルドする（`scripts/build-mediaremote-adapter.sh`、cmakeが要る）。フレームワークはリンクしない
- 起動時にadapterの`test`を実行し、失敗したら機能を使えない状態にして理由をメニューに出す（黙って落とさない）
- 子プロセスは常駐し、出力が届いたときだけ処理する。`--no-artwork`でアートワークの運搬を省く。
  前回が`stop()`を通らず落ちて残ったperlは、起動時に同じ引数のものを`pkill`で片付ける
- MediaRemoteは「選ばれた1つ」しか報告しない。Apple Musicが選ばれている間のYouTube Musicは見えない

### 判定

`YouTubeMusic.matches`が両OS共通の規則（`spec/youtube_music_vectors.json`）:
ブラウザで、アルバム名があり、再生元のブラウザに`music.youtube.com`のタブがある。タブを読めない
Windowsは、タブの条件を外してアルバム名だけで決める。

macOSだけの補強として、アルバム名が空でも、再生中の曲名がYouTube Musicのタブのタイトル
（「曲名 | YouTube Music」）と一致すれば拾う（ページを読み込み直した直後の曲にはアルバム名が付かない）。
通常のYouTube動画のタブは別のサフィックスなので一致しない。

- タブは`BrowserTabs`がAppleScriptで読む。新しい曲が来たときだけで、同じ曲のあいだは前回の結果を使い回す。
  読めるブラウザの許可リストは`YouTubeMusic.appleScriptBrowserBundleIds`（Chromium系・Safari系）と`firefoxProfileDirectories`（Firefox系）
- Firefox系（Firefox・Zen・Floorp・LibreWolf・Waterfoxなど）はAppleScriptでタブを読めないので、`FirefoxSession`がブラウザごとの`Profiles/*/sessionstore-backups/recovery.jsonlz4`
  （mozlz4: マジック＋展開後サイズ＋フレームなしLZ4ブロック、`COMPRESSION_LZ4_RAW`で展開）を読む。
  起動中のプロファイルは決まらないので、いちばん新しく書かれたものを使う。同じファイル（更新時刻が同じ）は展開し直さない
- Firefoxでもアルバム名はページ次第で、読み込み直後の曲では空になり、判定は曲名とタブのタイトルの一致に頼る。ファイルは約15秒おきに書かれ、
  曲が替わってもしばらく前の曲のタブを指す（実測でタイトルの追従に最大約19秒）。そのため一致しないときは結果を
  使い回さず、4秒おきに最大8回読み直す。曲ボタンもタイトルが一致したタブからしか取らない（古いURLを出さない）
- Firefoxのプロファイルは「他のアプリのデータへのアクセス」（`kTCCServiceSystemPolicyAppDataDetailed`）で守られている。
  macOSはこの許可でダイアログを出さず、拒否して通知だけ出す。拒否はEPERMとして区別し、メニューで「ファイルとフォルダ」へ案内する。
  ad-hoc署名の開発版では許可がビルドのcdhashに紐づくため、作り直すたびに許可し直しが要る
- アーティストが空の更新（曲が替わった直後でメタデータが揃う前）は待つ。曲名だけでカタログを引くと別の曲に当たる
- YouTube Musicでなくなっても2.5秒は消さない。曲替わりでアルバム名が遅れて届くあいだの点滅を防ぐ

### 表示

- アートワークは、YouTube Music自身の検索（`YouTubeMusicSearch` / `core::ytmusic`）で引く。曲名とアーティストの
  一致が要り、長さは同点の決め手にだけ使う。言語は曲のかな・ハングル → メニューの言語 → 英語の順。見つからなければ
  iTunes Search API（地域のストア+US・JP。リンクは借りずアートワークだけ）、それも無い（答えを待つあいだも含む）ときは、
  曲ページの動画のサムネイル（`i.ytimg.com/vi/<id>/mqdefault.jpg`。320x180で黒帯がなく、どの動画にもある）。
  曲ページが分からなければ出さない。YouTube Music側が「訊けなかった」ときは、iTunesで見つからなくても確定した「無い」にせず再試行する
- 曲ボタンはタブのURL。複数のタブがあるときは曲名がタイトルと一致するもの。`/watch`以外のページ
  （ホームなど）を開いているときはURLが曲を指さないので、YouTube Musicの検索で見つかった曲のURLを使う（見つからなければ出さない）
- アーティスト/アルバムのボタンはApple Musicのページを指すので出さない。曲ボタンの既定ラベルは
  「Play on YouTube Music」（利用者が変えたラベルは保持）
