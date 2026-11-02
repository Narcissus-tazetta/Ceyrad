# spec/

macOSビルド（Swift）とWindowsビルド（Rust）が**両方とも**読むテストデータ。

Discordに送るActivityの組み立ては両OSに1本ずつ実装がある。片方だけ直しても
どちらのテストも緑のままなので、乖離は気づかれずにリリースまで届く。ここに期待値を
1つだけ置き、両方のテストがそれを読むことで、乖離そのものをCIで落とす。

| ファイル | 読む側 |
|---|---|
| `activity_vectors.json` | `Tests/CeyradTests/SharedVectorTests.swift` / `windows/tests/shared_vector_tests.rs` |
| `youtube_music_vectors.json` | `Tests/CeyradTests/YouTubeMusicVectorTests.swift` / `windows/tests/youtube_music_vector_tests.rs` |

## activity_vectors.json

`cases[]` の各要素が「この入力からはこのActivityが出る」という1件。

- `track.positionSampledAtUnix` と `nowUnix` は絶対時刻（UNIX秒）。
  タイムスタンプが実行時刻に依存しないよう、両実装とも`now`を引数で受け取る。
- `catalog` が `null` ならカタログ未解決（ローカル取り込み曲）。
- `settings` は省略したキーが既定値になる。
- `expected` は組み立て結果のJSON全体。キーの過不足も差分として落とす。
- `source` は省略すると `"appleMusic"`。`"youtubeMusic"` ではボタンの組み立てが変わる
  （曲ボタンの既定ラベルが「Play on YouTube Music」になり、リンク先が Apple Music のページである
  アーティスト/アルバムのボタンは出ない）。

## youtube_music_vectors.json

`cases[]` の各要素が「この入力なら YouTube Music と判定する/しない」という1件。

- `isBrowser` は再生元がブラウザか。ブラウザの見分け方は OS ごとに違う（macOS の bundle id /
  Windows の AUMID）ので、ここには入れず入力として渡す。
- `hasYouTubeMusicTab` は再生元のブラウザに `music.youtube.com` のタブがあるか。`null` は
  そのOSにタブ確認の手段がない（Windows）ことを表し、アルバムの有無だけで決める。

## 期待値を変えるとき

**それが仕様変更であることを確かめてから**変える。
テストを通すために書き換えるのは、この仕組みの意味を消す行為になる。
