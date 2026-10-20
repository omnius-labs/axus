# No deadline for send_message and an accept-waiting Session after establishment

**Severity: Medium** (becomes visible when more callers stop applying deadlines)

Investigated on 2026-10-07 / Target commit `9bbbcbd`

One item of [issues.md](../issues.md).

## Symptoms

A Session that finished its handshake stays in `SessionAccepter`'s channel without a deadline.
The post-establishment `send_message` and `recv_message` have no deadline mechanism, and a caller without a timeout leaves a task waiting forever for a silent peer's reception.

## Location

`accept` at [accepter.rs:70](../../daemon/modules/engine/src/core/session/accepter.rs#L70) waits for channel reception without a deadline.

```rust
receiver.recv().await.ok_or_else(|| Error::new(ErrorKind::EndOfStream).with_message("receiver is closed"))
```

`FramedRecvExt::recv_message` at [packet.rs:9](../../daemon/modules/engine/src/base/connection/stream/packet.rs#L9) and `FramedSendExt::send_message` at [the same file:26](../../daemon/modules/engine/src/base/connection/stream/packet.rs#L26) only enforce the frame length limit and have no notion of time.
`SessionOption` at [model.rs:24](../../daemon/modules/engine/src/core/session/model.rs#L24) holds only the handshake deadline (`handshake_timeout`) and has no post-establishment deadline item.

## Cause

The handshake deadline is managed collectively in `SessionOption`, but post-establishment reception and the accept wait have no such mechanism.
Deadlines are left to each caller; NodeFinder's communicator applying a receive deadline (the design decision in [session.md](../design/session.md)) is an individual measure, not a common mechanism.

## Impact

A Session with a silent peer keeps its connection and receive task.
Accept-waiting Sessions pile up to the channel capacity (20), after which handshake-completed Sessions jam.
A future caller that forgets a deadline reproduces the same problem individually.
Confirmed by reading the code. Not reproduced by execution.

## Proposed fix

Add a post-establishment receive deadline to `SessionOption` and build the deadline into the recv/send of the Session or framed stream.
Also impose a deadline on the accept wait or sweep out unaccepted Sessions.
Replace the receive deadline NodeFinder's communicator applies individually with the common mechanism.
