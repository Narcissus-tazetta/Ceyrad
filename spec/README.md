# spec/

macOSビルド（Swift）とWindowsビルド（Rust）が**両方とも**読むテストデータ。

Discordに送るActivityの組み立ては両OSに1本ずつ実装がある。片方だけ直しても
どちらのテストも緑のままなので、乖離は気づかれずにリリースまで届く。ここに期待値を
1つだけ置き、両方のテストがそれを読むことで、乖離そのものをCIで落とす。

| ファイル | 読む側 |
|---|---|
| `activity_vectors.json` | `Tests/CeyradTests/SharedVectorTests.swift` / `windows/tests/shared_vector_tests.rs` |
| `youtube_music_vectors.json` | `Tests/CeyradTests/YouTubeMusicVectorTests.swift` / `windows/tests/youtube_music_vector_tests.rs` |
| `youtube_music_search_vectors.json` | `Tests/CeyradTests/YouTubeMusicSearchVectorTests.swift` / `windows/tests/ytmusic_search_vector_tests.rs` |
| `advertisement_vectors.json` | `Tests/CeyradTests/AdvertisementVectorTests.swift` / `windows/tests/advertisement_vector_tests.rs` |

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

## youtube_music_search_vectors.json

YouTube Musicの検索（`YouTubeMusicSearch` / `core::ytmusic`）の純粋な部分。

- `languages[]`: 曲とアプリの表示言語から、検索に使う言語の順序。
- `picks[]`: 検索結果の行（`byline` はアーティスト・アルバム・長さを `•` でつないだrunの列）から、
  この曲にあたる行の `videoId`（無ければ `null`）。`expectedSongURL` / `expectedArtworkURL` は
  選ばれた行から組み立てるカタログのリンク。

## advertisement_vectors.json

`cases[]` の各要素が「この曲情報はApple Musicの広告とみなす/みなさない」という1件。
広告中の通知を実測したものではなく、Discordに出た表示から推測した条件。

## 期待値を変えるとき

**それが仕様変更であることを確かめてから**変える。
テストを通すために書き換えるのは、この仕組みの意味を消す行為になる。
