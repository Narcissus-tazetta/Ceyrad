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
  - Catalog URLが解決できない曲（ローカル取り込み等）は自動的にリポジトリURLへフォールバック
- Apple Musicが**起動していないときは一切動作しない**（監視・接続・通知購読すべて停止）

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
- MediaRemote私的フレームワークはmacOS 15.4以降エンタイトルメント制限があるため使わない
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
- 国コードはシステムロケールから取得（`Locale.current.region`、fallback `US`）

### 4.5 ActivityBuilder

- **`type: 2`（Listening）** を指定 →「Apple Music を再生中」表示になる
- `details` = 曲名、`state` = アーティスト、`assets.large_text` = アルバム
  （Discord仕様: 各2〜128文字。短い場合はパディング）
- タイムスタンプ: `start = now - position`、`end = start + duration`（ms）。再生中のみ付与
- ボタン: 最大2個・label≤32文字・url≤512文字・http(s)のみをバリデーション。
  URL重複時は2個目を捨てる。Catalog解決失敗時はリポジトリURLへフォールバック
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
  - `customURL` / `repositoryURL`（フォールバック先）
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
| Catalog解決 | iTunes Search API。失敗もキャッシュし、ボタンはリポジトリURLへフォールバック |
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
3. iTunes Search APIのマッチングは一致ベースのため、表記揺れが大きい曲は解決できないことがある（feat.表記差・「- Single」サフィックスは正規化で吸収済み）→ 残りはフォールバックで吸収
4. ステータスに出す名称（"Apple Music" など）はDiscord Developer Portalで作るApplicationの名前で決まる
