# A vanished destination directory of Failed or Canceled rows makes sweep retries continue and remove impossible

**Severity: Medium** (only after the destination directory is deleted)

Investigated 2026-11-12 / Target commit `6068650`

One item of [issues.md](../issues.md).

## Symptom

When the destination directory of a Failed or Canceled row is gone (such as when an external disk is removed), the following two continue.

- The sweep fails each run, and the retries never end
- `remove` fails, and the row cannot be deleted

## Location

`delete_temporary_output` at [store.rs:417](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L417) treats a temporary output's `NotFound` as "absent" and then checks the destination directory's existence with `metadata`. When the directory is gone, `metadata` errors, and `delete_temporary_output` returns the error.

```rust
if !tokio::fs::metadata(&file.output_directory).await?.is_dir() {
    return Err(Error::new(ErrorKind::IoError).with_message("output parent is not a directory"));
}
```

`sweep_outputs_locked` at [store.rs:477](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L477) calls `delete_temporary_output` for the Canceled and Failed rows, and when it returns an error, schedules a retry with `request_sweep`. The next sweep hits the same error.
`remove` at [store.rs:185](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L185) calls `discard_output` (at [the same file:402](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L402)), so the same error prevents row deletion.

## Cause

The temporary output deletion and the destination directory existence check happen in the same function, and a vanished directory is returned as a recoverable error.
PR #120 fixed cancel and remove of Completed rows and rows whose temporary output cannot be created because the name is too long, but both sweep and remove of Failed and Canceled rows hit the same failure.

## Impact

While the destination directory is kept deleted, the daemon repeatedly logs the error and the row cannot be removed by any operation.
Restoring the disk recovers.
Confirmed by reading the code. Not reproduced by execution.

## Remedy

For Failed and Canceled rows, treat a temporary output's absence when the destination directory is vanished as "already deleted" (if the whole directory is gone, the temporary output is gone too).
Limit the destination directory existence check to the operations that need to protect an existing output (writing and finalizing).
Test that `remove` succeeds with the destination directory gone and that the sweep stops retrying.
