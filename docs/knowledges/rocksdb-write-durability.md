# RocksDB power-loss durability depends on the sync flag and build-time definitions

**Source: verified** (rocksdb 0.24.0 / librocksdb-sys 0.17.3+10.4.2 / cc 1.2.56 / aarch64-apple-darwin / 2026-10-06)

One item of [knowledges.md](../knowledges.md).

## Facts

RocksDB's `WriteOptions.sync` defaults to false, and even if a write reaches the OS buffer at the time of success, an OS crash or power loss can lose the most recent updates.
With WAL enabled, ordinary asynchronous writes that do not use manual WAL flush are recoverable on a process exit after the write finishes.
`sync=true` involves an OS sync, but the actual durability guarantee depends on the filesystem and device honoring the sync.
rust-rocksdb's `TransactionDB::transaction()` uses the default WriteOptions.

The RocksDB POSIX implementation uses `fcntl(F_FULLFSYNC)` to sync files only when built with `HAVE_FULLFSYNC` defined; otherwise it uses `fdatasync` or `fsync`.
RocksDB's CMake and `build_tools/build_detect_platform` add this definition in environments where `F_FULLFSYNC` is available.
librocksdb-sys 0.17.3's `build.rs` builds with `cc` instead of CMake, and for macOS defines `OS_MACOSX` but not `HAVE_FULLFSYNC`.
Therefore, with the default build of this version, specifying `sync=true` on macOS does not sync the device cache.

cc 1.2.56 reads `CXXFLAGS_<target>` for C++ builds and also accepts a name with the target's hyphens replaced by underscores.
Setting `CXXFLAGS_aarch64_apple_darwin` and `CXXFLAGS_x86_64_apple_darwin` to `-DHAVE_FULLFSYNC` in Cargo's `[env]` passes the definition to the librocksdb-sys macOS build.
These two variables are not used for the Linux and Windows target flags.
`force=true` makes the Cargo configuration value override a same-named environment variable.

## Evidence

Consulted [RocksDB Basic Operations: Synchronous Writes](https://github.com/facebook/rocksdb/wiki/Basic-Operations#synchronous-writes) and [Non-sync Writes](https://github.com/facebook/rocksdb/wiki/Basic-Operations#non-sync-writes) on 2026-10-04.
The Rust wrapper defaults were confirmed in [rocksdb 0.24.0 WriteOptions::set_sync](https://docs.rs/rocksdb/0.24.0/rocksdb/struct.WriteOptions.html#method.set_sync) and in the same version's distributed source `transactions/transaction_db.rs` and `db_options.rs`.
The `HAVE_FULLFSYNC` handling was confirmed in the librocksdb-sys 0.17.3+10.4.2 distributed source: `rocksdb/env/io_posix.cc` lines 1452-1477 (`PosixWritableFile::Sync` and `Fsync`), `rocksdb/CMakeLists.txt` lines 576-578, and `build_rocksdb` in `build.rs` (from line 50).
The per-target variable selection of cc 1.2.56 was confirmed in its distributed source `src/lib.rs`, `target_envs` (from line 3886) and `envflags` (from line 3922).
In an aarch64-apple-darwin build with the two variables set in Cargo's `[env]`, `CXXFLAGS_aarch64_apple_darwin = Some(-DHAVE_FULLFSYNC)` appeared in `target/debug/build/librocksdb-sys-*/output`.
The `out/*-io_posix.o` of the same build was examined with the following commands.

```sh
otool -tvV target/debug/build/librocksdb-sys-*/out/*-io_posix.o
otool -rv target/debug/build/librocksdb-sys-*/out/*-io_posix.o
```

`PosixWritableFile::Sync` and `Fsync` had an instruction putting `0x33` into the argument `w1` and a relocation for a `_fcntl` call.
Since `F_FULLFSYNC` is 51 (`0x33`) in the macOS SDK's `sys/fcntl.h`, both methods were confirmed to include an `fcntl(F_FULLFSYNC)` branch.
The x86_64-apple-darwin build and the Linux and Windows builds were not run.
No crash or power-loss tests were run.

## Implications

[storage.md §4.2](../design/storage.md#42-durability-guarantees-and-boundaries) and [§4.3](../design/storage.md#43-publishing-commit-and-reclamation) persist block saves and logical key changes before finalizing the metadata.
§4.2 assumes building RocksDB on macOS with `HAVE_FULLFSYNC` defined.
