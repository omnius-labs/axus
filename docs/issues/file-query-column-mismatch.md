# SQL が存在しない列 `property` を参照している

**深刻度: 高**（schema 初期化の不具合により現時点では未顕在）

調査日 2026-09-02 / 対象コミット `8970f441df9629246e5c6b536334683f79f3953a`

[issues.md](../issues.md) の 1 項目。

## 症状

file 情報を取得または挿入する query が `no such column: property` で失敗する。

## 該当箇所

どの対象 table にも `property` 列はない。

| file | 該当行 | 対応する列 |
| --- | --- | --- |
| `publisher_repo.rs` | [111](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L111)、[125](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L125) | `attrs` |
| `publisher_repo.rs` | [301](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L301)、[330](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L330) | `attrs` |
| `subscriber_repo.rs` | [91](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L91)、[106](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L106)、[121](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L121)、[138](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L138) | `priority` |

subscriber 側では `attrs` が別に選択されているため、`property` は `priority` の位置にある。
publisher 側では `property` が `attrs` の位置にある。
[subscriber_repo.rs:308](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L308) の `INSERT` は `attrs` と `priority` を使っている。

## 原因

query の列名が table schema および model と一致していない。

## 影響

[file-schema-initialization-fails.md](./file-schema-initialization-fails.md) を修正して repository を初期化できても、対象 query は失敗する。
file 公開と購読の処理を継続できない。

## 対応方針

publisher 側の `property` を `attrs` に置換する。
subscriber 側の `property` を `priority` に置換する。
schema の `depth` と `rank` の不一致も同時に修正する。
再発防止として、`sqlx::query!` による compile 時検証を導入できるか検討する。
