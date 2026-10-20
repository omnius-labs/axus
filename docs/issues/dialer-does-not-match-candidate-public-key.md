# The dialing side does not reconcile a candidate's public key with the Session cert

**Severity: Medium** (visible when a signed NodeProfile's reachable address becomes another node's)

Investigated on 2026-10-12 / Target commit `4bb0571`

One item of [issues.md](../issues.md).

## Symptoms

When connecting to a NodeProfile's reachable address and establishing a Session, the dialing sides of NodeFinder and FileExchanger record the connection as being to the NodeProfile's node without checking the peer's public key.
Even if the address now belongs to another node, the connection is handled as success as long as that node establishes the Session with its own key.

## Location

[node/task_connector.rs:157](../../daemon/modules/engine/src/core/negotiator/node/task_connector.rs#L157) connects to the candidate's reachable address, and on success [node/task_connector.rs:166](../../daemon/modules/engine/src/core/negotiator/node/task_connector.rs#L166) adds the candidate's NodeProfile as connected.

```rust
if let Ok(session) = self.session_connector.connect(addr, &SessionType::NodeFinder).await {
```

[file/task_connector.rs:214](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L214) and [file/task_connector.rs:222](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L222) have the same shape.
Neither compares the public key of `session.cert` with the public key of the candidate's NodeProfile.

## Cause

The Session verifies the peer's signature and key, but the dialing side does not receive the public key it expected from the peer.
The reconciliation at [node/task_communicator.rs:172](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L172) compares the public key of the NodeProfile the peer sent with the peer's own cert, not with the candidate's public key.

## Impact

The NodeProfile signature keeps a third party from attaching its own reachable addresses to a node's public key and distributing them ([trust-security.md](../design/trust-security.md#nodeprofile-binds-reachable-addresses-with-the-nodes-own-signature)).
However, the reachable addresses of a signed NodeProfile can become stale.
After a node changes its address, if another node uses that address, the dialing side establishes a Session with the other node at that address and removes the original node from the connection candidates as connected.
The other node cannot be authenticated as the genuine node, because it cannot produce the genuine node's signature.
Because the NodeFinder Session is registered with the node ID of the NodeProfile the peer sent ([node/task_communicator.rs:133](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L133)), it is handled as the third-party node's own Session.

## Proposed fix

After a successful connection, confirm that `session.cert`'s public key matches the candidate's NodeProfile public key, and on a mismatch close the Session and try the next reachable address.
Guard with a test that a node with a different public key answering at the candidate's address is not added to the connected candidates.
