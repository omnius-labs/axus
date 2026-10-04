# RocksDB の電源断耐久性は sync 指定と build 時の定義に依存する

**出所: 文献のみ**（rocksdb 0.24.0 / librocksdb-sys 0.17.3+10.4.2 / RocksDB Basic Operations / 配布ソース / 2026-10-04）

[knowledges.md](../knowledges.md) の 1 項目。

## 事実

RocksDB の WriteOptions の `sync` は既定で false であり、書き込み成功時に OS の buffer まで届いていても、OS クラッシュや電源断で直近の更新を失い得る。
WAL を有効にし、手動 WAL flush を使わない通常の非同期書き込みは、書き込みが終わった後の process の終了では回復できる。
`sync=true` は OS の同期処理を伴うが、実際の保持保証は filesystem と device が同期を守ることに依存する。
rust-rocksdb の `TransactionDB::transaction()` は既定の WriteOptions を使う。

RocksDB の POSIX 実装は、`HAVE_FULLFSYNC` が定義されて build された場合だけ、file の同期に `fcntl(F_FULLFSYNC)` を使い、それ以外では `fdatasync` または `fsync` を使う。
RocksDB の CMake と `build_tools/build_detect_platform` は、`F_FULLFSYNC` が使える環境でこの定義を加える。
librocksdb-sys 0.17.3 の `build.rs` は CMake を使わず `cc` で build し、macOS 向けに `OS_MACOSX` は定義するが `HAVE_FULLFSYNC` は定義しない。
このため、この版の既定の build では、macOS で `sync=true` を指定しても device cache の同期は行われない。

## 根拠

[RocksDB Basic Operations: Synchronous Writes](https://github.com/facebook/rocksdb/wiki/Basic-Operations#synchronous-writes) と [Non-sync Writes](https://github.com/facebook/rocksdb/wiki/Basic-Operations#non-sync-writes) を 2026-10-04 に参照した。
Rust wrapper の既定値は [rocksdb 0.24.0 WriteOptions::set_sync](https://docs.rs/rocksdb/0.24.0/rocksdb/struct.WriteOptions.html#method.set_sync) と、同版の配布ソースの `transactions/transaction_db.rs` および `db_options.rs` で確認した。
`HAVE_FULLFSYNC` の扱いは librocksdb-sys 0.17.3+10.4.2 の配布ソースの `rocksdb/env/io_posix.cc` 1452–1477 行（`PosixWritableFile::Sync` と `Fsync`）、`rocksdb/CMakeLists.txt` 576–578 行、`build.rs` の `build_rocksdb`（50 行から）で確認した。
`build.rs` に与える `CXXFLAGS` などの環境変数で定義を加えられるかは確認していない。
クラッシュと電源断の試験は行っていない。

## 含意

[storage.md §4.2](../design/storage.md#42-永続化の保証と境界) と [§4.3](../design/storage.md#43-公開の-commit-と回収) は、block の保存・論理 key の変更を永続化してから metadata を確定する。
§4.2 は、macOS で RocksDB を `HAVE_FULLFSYNC` を定義して build することを前提にする。
