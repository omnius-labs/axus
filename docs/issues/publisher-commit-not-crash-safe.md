# commit の途中で落ちた file は再起動後に公開できなくなる

**深刻度: 高**（FileExchanger が未結線のため daemon では未顕在）

調査日 2026-09-25 / 対象コミット `59054a2`

[issues.md](../issues.md) の 1 項目。

## 症状

公開処理の commit 中に process が終了すると、再起動後に同じ file を符号化し直した処理が `AlreadyExists` で失敗し、file は Failed のまま公開されない。
RocksDB には、どの metadata からも参照されない committed の block が残る。

## 該当箇所

[task_encoder.rs:305](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L305) は block ごとに別の RocksDB transaction で key を committed へ rename し、その後に [task_encoder.rs:309](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L309) で SQLite の commit を行う。

```rust
self.blocks_storage.rename_key(old_key.as_str(), new_key.as_str(), false).await?;
```

符号化中の file は Pending のままであり（[publisher_repo.rs:317](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L317)）、再起動後にもう一度符号化される。
`overwrite` が `false` の rename は、移動先の key があると [key_value_rocksdb_storage.rs:83](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L83) で `AlreadyExists` を返す。

## 原因

commit が RocksDB と SQLite の 2 つの storage にまたがるのに、途中で終了した状態を回復する手順がない。
再起動時の [task_encoder.rs:125](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L125) の cleanup は uncommitted の block だけを消し、rename 済みの committed の key を扱わない。

## 影響

rename を 1 つでも終えた後、SQLite の commit を終える前に終了すると、その file は再起動後に公開できなくなる。
committed の key を先に置いた storage で同じ file を公開すると Failed になり、理由が `AlreadyExists` であることを一時的な test で確かめた。
FileExchanger が未結線のため、daemon ではまだ公開処理が動かない。

## 対応方針

rename を冪等にするか、再起動時に SQLite に行のない committed の key を消してから符号化を再開する。
どちらの場合も、commit の途中で終了した状態から再起動して同じ file を公開できることと、参照されない block が残らないことを test する。
[storage.md](../design/storage.md#4-論理-key-と-migration) の原子性の前提は単一の rename についてのものであり、2 つの storage をまたぐ commit の順序と回復手順もそこに書き足す。
