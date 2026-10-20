# SessionConnector accepts the Unknown use-selection result

**Severity: Low** (only when the accepting side returns `Unknown`; the current SessionAccepter returns only `Accept` and `Reject`)

Investigated on 2026-10-11 / Target commit `794035e`

One item of [issues.md](../issues.md).

## Symptoms

When the accepting side returns `V1ResultType::Unknown` for a use request, the dialing side passes the Session up as accepted.

## Location

[connector.rs:94](../../daemon/modules/engine/src/core/session/connector.rs#L94) returns an error only when the result is `Reject`.

```rust
if received_session_result_message.result_type == V1ResultType::Reject {
```

`V1ResultType` at [message.rs:28](../../daemon/modules/engine/src/core/session/message.rs#L28) has the three values `Unknown`, `Accept`, and `Reject`.

## Cause

The acceptance condition is written as "not `Reject`" instead of "is `Accept`".

## Impact

A Session the accepting side answered `Unknown` is treated as established only by the dialing side.
Because SessionAccepter at [accepter.rs:277](../../daemon/modules/engine/src/core/session/accepter.rs#L277) returns `Reject` for a request it does not accept, this does not happen between Axus nodes.
The side returning `Unknown` is limited to a different implementation or a peer that changed the value intentionally.
The peer is authenticated through the secure channel.
A NodeFinder Session whose peer does not respond is closed by the receive deadline at [task_communicator.rs:177](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L177).
A FileExchanger Session has no post-establishment send/receive loop and deadline ([session-accept-and-message-without-deadline](./session-accept-and-message-without-deadline.md)) and remains handed to the channel at [task_connector.rs:214](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L214).

## Proposed fix

Return the Session only when the result is `Accept`, and error otherwise.
Guard with a test that the connection errors when the accepting side returns `Unknown`.