# output_lock is common across all subscriptions and a writing subscription stops other subscriptions' finalization

**Severity: Medium** (when a subscription with a long output exists, other subscriptions' finalization is delayed)

Investigated on 2026-10-07 / Target commit `9bbbcbd`

One item of [issues.md](../issues.md).

## Symptoms

While one subscription writes its temporary output, the output operations of all other subscriptions (finalize, discard_output, sweep) wait for the lock to be released.
When a large file subscription is writing its output, the finalization of other completed subscriptions is stalled for a long time.

## Location

As [store.rs:26](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L26) shows, there is one `FileSubscriberStore` per `state_dir`, and only one `output_lock`.

```rust
output_lock: Arc<TokioMutex<()>>,
```

`create_output` at [store.rs:202](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L202) hands the lock guard to the `OutputWriter` (at [the same file:469](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L469)), which holds it until the output write finishes.
`finalize` at [store.rs:257](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L257), `discard_output` at [the same file:356](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L356), and `sweep_outputs_locked` at [the same file:431](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L431) wait on the same lock.
As [file_subscriber.rs:31](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber.rs#L31) shows, `FileSubscriber` holds one store shared by all subscriptions.

## Cause

The destination conflict between subscriptions (protecting the same destination) is guarded by a single lock across all subscriptions.
The lock granularity is coarser than the subscription (row).

## Impact

Output never stalls, but the time from a subscription's completion to its finalization is dragged by other subscriptions' output time.
When long-output and short-output subscriptions coexist, the short ones' finalization is delayed by a long time.
Confirmed by reading the code. Not reproduced by execution.

## Proposed fix

Change the lock granularity to per-subscription (per-destination).
Make the guard held by `OutputWriter` a per-destination lock, and make `finalize` and `discard_output` take only that subscription's lock.
Test that one subscription's write does not block another subscription's `finalize` from completing.
