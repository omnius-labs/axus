# delete_bulk succeeds deleting nothing when it contains a missing key

**Severity: Low** (not visible because the current callers pass only existing keys)

Investigated on 2026-10-06 / Target commit `8432b83`

One item of [issues.md](../issues.md).

## Symptoms

When `KeyValueRocksdbStorage::delete_bulk` contains at least one missing key, it returns success without writing anything, including the deletions queued before that key.
A key that was not deleted looks deleted to the caller.

## Location

[key_value_rocksdb_storage.rs:430](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L430) `return Ok(())`s on a key whose name could not be resolved to an internal ID, never reaching the `write_opt` at [the same file:438](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L438).

```rust
None => return Ok(()),
```

## Cause

Skipping a missing key needs `continue`, but the code is written as `return Ok(())`, so the batch write is never reached.
`test_delete_bulk` checks separately that passing only missing keys does not error and that passing only existing keys deletes them, but does not check the mixed case.

## Impact

The only caller, `delete_prefix` ([store.rs:189](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L189)), passes only the keys enumerated inside the same mutex, so it is not affected at this point.
Adding a call that mixes existing and missing keys leaves undeleted keys.

## Proposed fix

Skip missing keys with `continue` and delete the rest in one batch.
Test that a call mixing existing and missing keys deletes all the existing keys.
