# file の I/O 完了だけでは内容と directory entry の永続化は揃わない

**出所: 文献のみ**（Tokio 1.52.3 / Rust std 1.96.0 ソース / Linux man-pages 6.19 / Apple fsync(2) archived manual / 2026-10-04）

[knowledges.md](../knowledges.md) の 1 項目。

## 事実

Tokio の File の `flush` は buffered I/O の完了を待つが、file 内容の永続化を保証しない。
Tokio の `sync_all` は blocking thread で std の `sync_all` を呼ぶ。
std の `sync_all` は、Linux では `fsync`、macOS などの Apple 製 OS では `fcntl(F_FULLFSYNC)`、Windows では `FlushFileBuffers` を呼ぶ。

Linux の `fsync(file)` は file の内容と metadata を同期するが、親 directory の entry の同期まで必ず行うわけではない。
entry の保持には directory の file descriptor に対する `fsync` が必要である。
Unix では directory を std の `File` として開き、`sync_all` を呼べる。

Apple の fsync(2) manual は、`fsync` 後も device の cache に書き込みが残り得るとし、厳しい保持・順序保証には `F_FULLFSYNC` を示す。
API の成功が device の故障まで保証するものではない。
OS と保存先が要求する同期を守ることが前提である。

## 根拠

[Tokio 1.52.3 File](https://docs.rs/tokio/1.52.3/tokio/fs/struct.File.html#method.sync_all) と同版の配布ソース `src/fs/file.rs` の `sync_all`、[Linux man-pages 6.19 fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html)、[Apple fsync(2)](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html) を 2026-10-04 に参照した。
std の呼び出し先は、`daemon/rust-toolchain.toml` が固定する Rust 1.96.0 の配布ソース `library/std/src/sys/fs/unix.rs` の `fsync` と `library/std/src/sys/fs/windows.rs` の `fsync` で確認した。
Apple の資料は archived manual であり、対象の macOS/filesystem での同期可否は別途確認が必要である。
device cache や電源断の実機試験は行っていない。

## 含意

[storage.md §4.2](../design/storage.md#42-永続化の保証と境界) と [§4.5](../design/storage.md#45-出力の確定と回復) は、一時出力の `sync_all` と、Linux と macOS での親 directory の `sync_all` を Completed の前提にする。
Windows の directory entry は、std の `sync_all` ではなく [置換しない rename](./filesystem-output-publication.md) の `MOVEFILE_WRITE_THROUGH` で永続化する。
