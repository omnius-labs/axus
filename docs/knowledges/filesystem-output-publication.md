# A non-replacing rename is provided by a different API on each OS

**Source: literature and runtime verification** (Linux man-pages 6.19 / macOS 26.6.2 rename(2) / Microsoft Learn MoveFileExW and MoveFile / Rust std 1.96.0 source / 2026-10-04, 2026-10-07)

One item of [knowledges.md](../knowledges.md).

## Facts

Rust's `std::fs::rename` maps to `rename` on Unix and `MoveFileExW` or `SetFileInformationByHandle` on Windows, and replaces an existing file's target.
A rename that fails when an existing entry is present is obtained, per OS, with the following APIs.

| OS | API | When the target exists | When the store does not support it |
| --- | --- | --- | --- |
| Linux | `renameat2` with `RENAME_NOREPLACE` | Fails with `EEXIST` | Fails with `EINVAL` |
| macOS | `renamex_np` with `RENAME_EXCL` | Fails with `EEXIST` | Fails with `ENOTSUP` |
| Windows | `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` | Fails | Not documented |

Per man-pages, Linux's `RENAME_NOREPLACE` is supported by ext4 (Linux 3.15), btrfs, tmpfs, and cifs (3.17), xfs (4.0), and ext2, vfat, and others (4.9).
macOS's `RENAME_EXCL` is effective on filesystems with `VOL_CAP_INT_RENAME_EXCL` in `getattrlist(2)`.
Windows's `MoveFileExW` with `MOVEFILE_WRITE_THROUGH` does not return until the move is reflected to disk.
The handling of an existing target without `MOVEFILE_REPLACE_EXISTING` can be read as the opposite of the flag's description in the `MoveFileExW` documentation, and the `MoveFile` documentation states that the target must not exist.

For any API, a rename renames one name to another within the same filesystem or volume.
Specifying Windows's `MOVEFILE_COPY_ALLOWED` makes a cross-volume move a copy and delete, which is no longer a rename.

On the filesystem of macOS 26.6.2 `/private/tmp`, a placement with `renamex_np`'s `RENAME_EXCL` succeeded, the source name disappeared, and the content remained at the target.
With a regular file, directory, symlink, or dangling symlink at the target, all failed with `EEXIST`, keeping the source and the existing content.

## Evidence

Consulted [Linux man-pages 6.19 rename(2)](https://man7.org/linux/man-pages/man2/rename.2.html), macOS 26.6.2 `man 2 rename`, [Microsoft Learn MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw), and [Microsoft Learn MoveFile](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefile) on 2026-10-04.
The `std::fs::rename` mapping was confirmed in the documented comments of `library/std/src/fs.rs` in the Rust 1.96.0 distributed source.
On 2026-10-07, macOS `renamex_np` was called from Python's `ctypes`, and the placement and the rejection of the 4 kinds of existing entries were confirmed in a temporary directory.
The placement test of the engine was also run on macOS, confirming the same existing-entry protection and the success of the parent-directory sync.
Execution on Linux and Windows and power-loss tests are not confirmed.

## Implications

[storage.md §4.5](../design/storage.md#45-output-finalization-and-recovery) places the temporary output with a non-replacing rename and judges placement from the presence of the temporary output and the destination.
A failure when the store does not support it is handled as Failed in §4.5.
On Windows, the directory entry persistence depends on `MOVEFILE_WRITE_THROUGH` in [storage.md §4.2](../design/storage.md#42-durability-guarantees-and-boundaries).