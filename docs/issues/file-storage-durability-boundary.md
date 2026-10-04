# file の metadata 確定前に実体の電源断耐久性を確保していない

**深刻度: 中**（file 転送と REST API は未結線。engine の保存・完了後に OS クラッシュや電源断が起きた場合の保証不足）

調査日 2026-10-04 / 対象コミット `84784b2` を基点とする staged 変更後の作業ツリー

[issues.md](../issues.md) の 1 項目。

## 症状

block の書き込みと論理 key の rename の完了後に、SQLite の downloaded や committed file を確定するが、先行する RocksDB の同期書き込みを指定していない。
購読の出力も `flush` 後に Completed を記録し、file と親 directory の永続化を確認しない。
OS クラッシュや電源断では、確定した metadata が失われた実体を参照し得る。

## 該当箇所

[key_value_rocksdb_storage.rs:58](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L58) と [同ファイル:217](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L217) は既定の `db.transaction()` を使う。
[publisher/store.rs:183](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L183) は rename の後に SQLite を確定する。
[subscriber/task_decoder.rs:125](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L125) は直接出力して Completed を記録する。
[publisher/repo.rs:26](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs#L26) と [subscriber/repo.rs:23](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/repo.rs#L23) は WAL を指定するが、環境ごとの強い同期条件を明示していない。

## 原因

storage の処理完了を、電源断後にも保持できる永続化の完了と同じものとして扱っている。
[RocksDB の同期](../knowledges/rocksdb-write-durability.md)、[SQLite の同期](../knowledges/sqlite-wal-durability.md)、[file/directory の同期](../knowledges/filesystem-sync-durability.md) は、それぞれ別の条件を持つ。
macOS の RocksDB は、build に `HAVE_FULLFSYNC` が定義されていないため、`sync=true` を指定しても `F_FULLFSYNC` を使わない。
SQLx の既定の FULL を使っていても、先行する RocksDB や出力 file の同期不足は補えない。

## 影響

保存済み進捗または公開済み root の block、Completed の出力が障害後に欠落し得る。
現在の起動時回復は orphan key の回収であり、失われた実体を復元しない。
この指摘はコードと公式仕様の照合による。OS クラッシュ・電源断の実機試験は未実施である。

## 対応方針

[storage.md §4.2](../design/storage.md#42-永続化の保証と境界) の同期条件を保存先ごとに実装する。
RocksDB の書き込みに `sync=true` を指定し、macOS では RocksDB を `HAVE_FULLFSYNC` を定義して build する。
block の永続化後に metadata を確定し、出力配置では file と directory entry の同期後に Completed を記録する。
同期失敗時は成功を返さず、確定前の状態を保持する。
対応環境ごとの同期設定と障害試験で保証を確認する。
