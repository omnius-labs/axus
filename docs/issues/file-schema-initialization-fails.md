# ファイル関連の SQLite schema が初期化時にエラーになる

**深刻度: 高**（FileExchanger が構築されないため現時点では未顕在）

調査日 2026-09-02 / 対象コミット `8970f441df9629246e5c6b536334683f79f3953a`

[issues.md](../issues.md) の 1 項目。

## 症状

`FilePublisherRepo::new` と `FileSubscriberRepo::new` が migration の適用時に error を返す。
そのため FilePublisher と FileSubscriber を構築できない。

## 該当箇所

publisher 側の index 定義は [publisher_repo.rs:82](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L82) にある。

```sql
CREATE INDEX IF NOT EXISTS index_file_id_rank_index_for_uncommitted_blocks
    ON committed_blocks (file_id, rank ASC, `index` ASC);
```

index 名は `uncommitted_blocks` 用だが、対象 table は [committed_blocks](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L52) である。
`committed_blocks` は `root_hash` を持つ一方で、`file_id` を持たない。

subscriber 側の `files` table は [subscriber_repo.rs:41](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L41) にある。

1. [subscriber_repo.rs:42](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L42) と [subscriber_repo.rs:54](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L54) が primary key を二重に宣言している。
2. [subscriber_repo.rs:54](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L54) が table に存在しない `file_name` を参照している。
3. [subscriber_repo.rs:45](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L45) は `depth` を定義するが、[SubscribedFile](../../daemon/modules/engine/src/core/negotiator/file/model/subscribed_file.rs#L6) の対応 field は `rank` である。
4. [subscriber_repo.rs:64](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L64) の index は `is_downloaded` を参照するが、`blocks` table の列名は `downloaded` である。

## 原因

migration の SQL と、対象 table および model の名前が一致していない。

## 影響

file 公開と購読の repository を初期化できない。
AxusService は NodeFinder だけを構築し FileExchanger を所有しないため、現在の通常起動では顕在化しない。

## 対応方針

対象 table と model に合わせて index、primary key、列名を修正する。
既存 MigrationRequest を直接修正できるかは、[storage.md](../design/storage.md#4-論理-key-と-migration) に従って判断する。
適用済み環境が存在しないことを確認できた場合に限り、既存 MigrationRequest を直接修正する。
適用済み環境が存在する場合、または存在しないことを確認できない場合は、新しい MigrationRequest を追加する。
修正後は publisher と subscriber の初期化 test を追加する。
