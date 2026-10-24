# SQLite WAL power-loss durability depends on the sync conditions

**Source: literature only** (SQLx 0.9.0 / libsqlite3-sys 0.30.1 / bundled SQLite 3.46.0 / SQLite PRAGMA documentation / 2026-10-04)

One item of [knowledge.md](../knowledge.md).

## Facts

In SQLite WAL mode, `synchronous=FULL` adds a WAL sync after each transaction commit and ensures durability after a power loss.
`NORMAL` may keep consistency but can lose committed transactions on an OS crash or power loss.
The guarantee assumes the VFS, filesystem, and device execute sync requests correctly.

On macOS, `fullfsync=ON` uses `F_FULLFSYNC` for syncing, and the default is OFF.
When ON, it is also used for checkpoints, and a separate `checkpoint_fullfsync` is unnecessary.
The default of the `synchronous` option in SQLx 0.9.0's SqliteConnectOptions is FULL.

## Evidence

Consulted [SQLite PRAGMA synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous), [fullfsync](https://www.sqlite.org/pragma.html#pragma_fullfsync), and [checkpoint_fullfsync](https://www.sqlite.org/pragma.html#pragma_checkpoint_fullfsync) on 2026-10-04.
The SQLx defaults were confirmed in the same version's distributed source `sqlx-sqlite/src/options/mod.rs` and [SqliteConnectOptions::synchronous](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html#method.synchronous).
The SQLite version is per the `SQLITE_VERSION` in the libsqlite3-sys 0.30.1 distributed header.
No power-loss tests were run.

## Implications

[storage.md §4.2](../design/storage.md#42-durability-guarantees-and-boundaries) requires the explicitly stated sync conditions for commits of states including Finalizing and Completed.
The durability of SQLite itself and the durability of the RocksDB or output files written earlier are secured separately.
