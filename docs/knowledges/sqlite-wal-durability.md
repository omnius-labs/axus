# SQLite の WAL の電源断耐久性は同期条件に依存する

**出所: 文献のみ**（SQLx 0.9.0 / libsqlite3-sys 0.30.1 / bundled SQLite 3.46.0 / SQLite PRAGMA 文書 / 2026-10-04）

[knowledges.md](../knowledges.md) の 1 項目。

## 事実

SQLite の WAL mode では `synchronous=FULL` が各 transaction commit 後に WAL の同期を加え、電源断後の durability を確保する。
`NORMAL` では整合性を保てても、OS クラッシュや電源断で確定済み transaction を失い得る。
保証は VFS、filesystem、device が同期要求を正しく実行することを前提とする。

macOS の `fullfsync=ON` は同期に `F_FULLFSYNC` を使う。既定値は OFF である。
ON の場合は checkpoint にも使われ、`checkpoint_fullfsync` の別指定は不要である。
SQLx 0.9.0 の SqliteConnectOptions の synchronous の既定値は FULL である。

## 根拠

[SQLite PRAGMA synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous)、[fullfsync](https://www.sqlite.org/pragma.html#pragma_fullfsync)、[checkpoint_fullfsync](https://www.sqlite.org/pragma.html#pragma_checkpoint_fullfsync) を 2026-10-04 に参照した。
SQLx の既定値は同版の配布ソースの `sqlx-sqlite/src/options/mod.rs` と [SqliteConnectOptions::synchronous](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html#method.synchronous) で確認した。
SQLite の版は libsqlite3-sys 0.30.1 の配布 header の `SQLITE_VERSION` による。
電源断の試験は行っていない。

## 含意

[storage.md §4.2](../design/storage.md#42-永続化の保証と境界) は、Finalizing と Completed を含む状態の commit に、明示した同期条件を要求する。
SQLite 自体の durability と、先に書いた RocksDB や出力 file の durability は別々に確保する。
