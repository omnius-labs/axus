# TaskCommunicator::start does not exit its loop even when the accept channel is closed

**Severity: Low** (only when the accept channel's sender is dropped)

Investigated 2026-11-12 / Target commit `6068650`

One item of [issues.md](../issues.md).

## Symptom

When the NodeFinder Session accept channel closes, `recv` immediately returns `None`, which repeats continuously.
The `TaskCommunicator::start` loop becomes a busy loop alternating cleanup of finished tasks and reception, and keeps using CPU unless stopped by cancellation.

## Location

[task_communicator.rs:116](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L116) Inside the loop, the `select!` at [the same file:124](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L124) does not break when `recv()` returns `None`, skips the `if let Some(status)` at [the same file:127](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L127), and returns to the top of the loop.

```rust
let status = select! {
    status = async { this.session_receiver.lock().await.recv().await } => status,
    _ = this.cancellation_token.cancelled() => break,
};
if let Some(status) = status {
```

## Cause

A `recv()` `None` (channel closure) is treated the same as "nothing was received".
Channel closure is a permanent condition and, unlike a missing reception, must break.

## Impact

In the current composition, the sender stays alive while NodeFinder is alive, so it is not visible.
A change in shutdown order or a dropped sender starts a busy loop until exit.
Confirmed by reading the code. Not reproduced by execution.

## Remedy

Break the loop when `recv()` returns `None` (a channel closure does not recover).
Test that the start task exits with the accept channel closed.
