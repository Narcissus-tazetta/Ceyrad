# spec/

macOSビルド（Swift）とWindowsビルド（Rust）が**両方とも**読むテストデータ。

Discordに送るActivityの組み立ては両OSに1本ずつ実装がある。片方だけ直しても
どちらのテストも緑のままなので、乖離は気づかれずにリリースまで届く。ここに期待値を
1つだけ置き、両方のテストがそれを読むことで、乖離そのものをCIで落とす。

| ファイル | 読む側 |
|---|---|
| `activity_vectors.json` | `Tests/CeyradTests/SharedVectorTests.swift` / `windows/tests/shared_vector_tests.rs` |

## activity_vectors.json

`cases[]` の各要素が「この入力からはこのActivityが出る」という1件。

- `track.positionSampledAtUnix` と `nowUnix` は絶対時刻（UNIX秒）。
  タイムスタンプが実行時刻に依存しないよう、両実装とも`now`を引数で受け取る。
- `catalog` が `null` ならカタログ未解決（ローカル取り込み曲）。
- `settings` は省略したキーが既定値になる。
- `expected` は組み立て結果のJSON全体。キーの過不足も差分として落とす。

期待値を変えるときは、**それが仕様変更であることを確かめてから**変える。
テストを通すために書き換えるのは、この仕組みの意味を消す行為になる。
