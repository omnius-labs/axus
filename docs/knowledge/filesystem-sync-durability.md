# File I/O completion alone does not align content and directory entry persistence

**Source: literature and runtime verification** (Tokio 1.52.3 / Rust std 1.96.0 source / Linux man-pages 6.19 / Apple fsync(2) archived manual / 2026-10-04, 2026-10-07)

One item of [knowledge.md](../knowledge.md).

## Facts

Tokio's File `flush` waits for buffered I/O to complete but does not guarantee the file content is persisted.
Tokio's `sync_all` calls std's `sync_all` on a blocking thread.
std's `sync_all` calls `fsync` on Linux, `fcntl(F_FULLFSYNC)` on Apple-made OSes such as macOS, and `FlushFileBuffers` on Windows.

Linux's `fsync(file)` syncs the file's content and metadata but does not always sync the parent directory's entry.
Retaining the entry requires `fsync` on the directory's file descriptor.
On Unix, a directory can be opened as a std `File` and `sync_all` called on it.

Apple's fsync(2) manual states that writes can remain in the device cache after `fsync` and points to `F_FULLFSYNC` for strict retention and ordering guarantees.
An API success does not guarantee durability up to device failure.
The OS and the destination honoring the requested sync is the premise.

For an open temporary directory at `/private/tmp` on macOS 26.6.2, `fcntl(F_FULLFSYNC)` was confirmed to succeed.

## Evidence

Consulted [Tokio 1.52.3 File](https://docs.rs/tokio/1.52.3/tokio/fs/struct.File.html#method.sync_all) and `sync_all` in the same version's distributed source `src/fs/file.rs`, [Linux man-pages 6.19 fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html), and [Apple fsync(2)](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html) on 2026-10-04.
The std callees were confirmed in the distributed source of the Rust 1.96.0 fixed by `daemon/rust-toolchain.toml`: `fsync` in `library/std/src/sys/fs/unix.rs` and `fsync` in `library/std/src/sys/fs/windows.rs`.
The Apple material is an archived manual, and whether the target macOS/filesystem supports the sync must be confirmed separately.
On 2026-10-07, `F_FULLFSYNC` was specified for a temporary directory descriptor with Python's `fcntl`, and the call's success was confirmed.
No device-cache or power-loss hardware tests were run.

## Implications

[storage.md §4.2](../design/storage.md#42-durability-guarantees-and-boundaries) and [§4.5](../design/storage.md#45-output-finalization-and-recovery) make the `sync_all` of the temporary output and the `sync_all` of the parent directory on Linux and macOS a prerequisite of Completed.
On Windows, the directory entry is persisted with the `MOVEFILE_WRITE_THROUGH` of the [non-replacing rename](./filesystem-output-publication.md), not with std's `sync_all`.
