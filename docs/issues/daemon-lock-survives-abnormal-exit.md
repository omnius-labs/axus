# Lock file of an abnormally terminated daemon remains and prevents restart

**Severity: Medium** (after `kill -9`, an OS crash, or a power loss, the daemon cannot start until the file is removed by hand)

Investigated on 2026-10-06 / Target commit `374c397`

One item of [issues.md](../issues.md).

## Symptoms

If the daemon does not exit normally and `axus.lock` remains, the next startup fails with an `AlreadyExists` error when acquiring the lock.
On a normal exit, the `FileLock` `Drop` deletes the file.

## Location

[file_lock.rs:23](../../daemon/refs/core-rs/modules/base/src/file_lock.rs#L23) `FileLock::acquire` creates the file with `create_new(true)`.
The process ID is written into the file but is not read when acquiring.

```rust
OpenOptions::new().write(true).create_new(true).open(path).await?
```

The daemon calls this `FileLock::acquire` at [executor.rs:113](../../daemon/entrypoints/daemon/src/executor.rs#L113).

## Cause

Exclusivity is judged only by the file's existence and is not tied to the process's liveness.
On an exit where `Drop` does not run, the file remains.

## Impact

An abnormally terminated daemon does not restart until the user deletes `axus.lock` by hand.
The error carries the lock path, so the file to delete is identifiable.
The same property held before the change, when the lock was placed in the configuration directory.

Confirmed by reading the code. Not reproduced by execution.

## Proposed fix

Change core-rs's `FileLock` to serialize with OS advisory locks (`flock` on Linux and macOS, `LockFileEx` on Windows).
Because the OS releases the lock when the process exits, the next acquire succeeds even if the file remains.
Test that acquisition fails while another process holds the lock and succeeds, with the file remaining, after `kill`ing that process.
Change core-rs first, and have axus update the submodule and use it.
