# On macOS the file exchanger tests are intermittently rejected at the handshake

**Severity: Medium** (the macOS run on the main branches fails; whether a daemon on macOS rejects a valid peer in the same way is not known)

Investigated on 2026-11-07 / Target commit `82dbb0e`

One item of [issues.md](../issues.md).

## Symptoms

On the macOS runner, tests of `file_exchanger::tests` fail with `kind: Reject` and `the peer rejected the root hash`, and every one panics at the same line.
Run 42347912979 failed `a_request_that_crosses_the_choke_is_ignored_and_a_request_after_the_unchoke_is_served`, and run 42349325499 failed `a_requested_hash_the_node_does_not_hold_is_answered_by_the_presence_bitmap` and `a_requester_exceeding_the_requests_awaiting_responses_is_excluded`.
Each macOS run failed a different set of these tests, and the Linux job passed in both runs.

## Location

[file_exchanger.rs:496](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L496) is the test peer's `ExchangeHandshake::dial` call, where the panic occurs.
The error is raised at [exchange_handshake.rs:28](../../daemon/modules/engine/src/core/negotiator/file/exchange_handshake.rs#L28) when the accepting node answers `Reject`.
The accepting node answers by [`can_exchange`](../../daemon/modules/engine/src/core/negotiator/file/exchange_context.rs#L155-L160), called from [task_accepter.rs:76](../../daemon/modules/engine/src/core/negotiator/file/task_accepter.rs#L76).

## Cause

Not investigated.
`can_exchange` is false for the node itself, an excluded peer, a root hash the node does not provide yet, and a root hash that already has the maximum number of Sessions; which of these held at the moment of the dial was not recorded.
That the failure depends on timing is a guess drawn from the different tests failing in each run.

## Impact

The macOS job of every `ci` run on a main branch fails, so that run cannot be green while this stays.
The failing side is the test peer dialing a real node, so a real peer might be rejected on macOS in the same way, which was not checked.

## Remedy

Reproduce it on macOS with the four conditions of `can_exchange` logged at the rejection, then fix the cause: a test that dials before the node provides the root hash waits for that state, and a rejection of a valid peer in the daemon is fixed in `ExchangeContext`.
