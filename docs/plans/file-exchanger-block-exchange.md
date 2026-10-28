# Plan: FileExchanger block exchange protocol and hash verification

## 1. About This Plan

This plan implements the block exchange of [file-transfer.md §5](../design/file-transfer.md#5-block-exchange) and the content hash verification on the subscriber side, and wires FileExchanger into the daemon startup path.
It also fixes the three confirmed bugs on the connection path that the new FileExchanger Session depends on.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| The protocol, constants, and the reasons for the decisions | [file-transfer.md](../design/file-transfer.md#5-block-exchange) and [session.md](../design/session.md#5-design-decisions) |
| Vocabulary | [terms.md](../terms.md) |
| Bugs fixed here | [issues.md](../issues.md) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-10-28 against commit `d7ea4d6`.

## 2. Purpose and Completion Criteria

Two daemons exchange the blocks of a published file and the subscriber reconstructs the file, and a block whose content hash does not match is never stored.
This is the first item of [design.md §6.2](../design.md#62-roadmap) and the prerequisite of the REST API.

### 2.1 Completion Criteria

1. A subscriber receives a multi-rank file from a publisher over loopback TCP and its output equals the published file.
2. A node in the middle of fetching hands the blocks it holds to a third node.
3. `write_block` stores only a block whose content hash matches, and reports stored, unneeded, and mismatch distinctly.
4. A peer that sends a mismatched block, an unrequested block, or exceeds a request limit is disconnected and excluded, and the subscription still completes from another peer.
5. Choke, request deadline, receive deadline, and the close of a mutually-unwanted Session behave as in §5.3 to §5.6 of the design.
6. `import` rejects a block size over 1 MiB, and the FileExchanger frame limit is 1 MiB plus the block message overhead.
7. An established Session has a receive deadline, an unaccepted Session is discarded after 10 seconds, and the dialing side rejects any use-selection result other than Accept.
8. The dialing side of NodeFinder and FileExchanger closes a Session whose cert public key differs from the candidate's NodeProfile.
9. `AxusService` starts and stops FileExchanger and accepts its use, and FileExchangers that find each other through real NodeFinders transfer a file.
10. `cargo make lint` and `cargo make test` pass.

### 2.2 Non-goals

| Item | Where it goes |
| --- | --- |
| REST API and any Rust entrypoint for publish and subscribe | Roadmap item 2 in [design.md](../design.md#62-roadmap) |
| Send bandwidth limit | [file-transfer.md §6.2](../design/file-transfer.md#62-deferred) |
| Persisting exclusions and transfer volumes | Rejected in [file-transfer.md §6.1](../design/file-transfer.md#61-decided) |

## 3. Current State

FileExchanger has only the connection and accept tasks and no message definition or send/receive loop ([file_exchanger.rs](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs)).
`FileSubscriber::write_block` saves without hashing and returns `()` ([file_subscriber.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber.rs)).
The FileExchanger frame limit is 64 MiB, and `import` only rejects a block size of 0.
`FramedStream::recv_message` has no deadline, and NodeFinder applies its own in [task_communicator.rs](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs).
`AxusService` creates only NodeFinder and accepts only the NodeFinder use ([service.rs](../../daemon/modules/engine/src/service.rs)).
Without these, a FileExchanger Session cannot carry blocks, and a tainted block would be stored.

## 4. Change Approach

The Session layer gains the common deadline first, then the Store gains verification and the provider entrances, and FileExchanger is rebuilt on top in the NodeFinder shape: a decision part (assignment, choke, exclusion) kept apart from per-Session send and receive workers.

### 4.1 Assumed Decisions

| Decision | Source |
| --- | --- |
| Protocol, constants, verification, and violations | [file-transfer.md §5](../design/file-transfer.md#5-block-exchange) |
| Receive deadline held by the stream and set by the use owner | [session.md](../design/session.md#post-establishment-receive-deadline) |
| Time constants injected through Intervals | [file-transfer.md](../design/file-transfer.md#inject-fileexchangers-time-constants-through-intervals) |
| Hash check at the Store entry | [file-transfer.md](../design/file-transfer.md#check-the-content-hash-at-the-store-entry) |

### 4.2 Scope of Impact

Touched: `base/connection/stream`, `core/session`, `core/negotiator/node`, `core/negotiator/file`, `service.rs`, `daemon/rpfs/file.rpf` and its generated code.
Affected without edits: NodeFinder behavior, since its receive deadline moves to the common mechanism with the same value.
The FileExchanger wire is new and the NodeFinder wire is unchanged, so no compatibility is broken.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [1](#s-1) | Common receive deadline and accept-wait deadline | | done |
| [2](#s-2) | Reject Unknown results and check the candidate's public key | | done |
| [3](#s-3) | Hash verification and provider entrances in the Store | | done |
| [4](#s-4) | Wire definition and frame limit | 3 | done |
| [5](#s-5) | Decision parts: assignment, choke, exclusion, Intervals | 4 | done |
| [6](#s-6) | Session registry, handshake, and per-Session worker | 1, 5 | done |
| [7](#s-7) | Connector, accepter, and location finder | 2, 6 | done |
| [8](#s-8) | AxusService wiring and the real-NodeFinder test | 7 | done |

<a id="s-1"></a>
### Step 1. Common receive deadline and accept-wait deadline

**Locations to change**
`base/connection/stream/framed.rs` and `packet.rs` (receive deadline), `core/session/accepter.rs` (accept wait), `core/negotiator/node/task_communicator.rs` (use the common deadline).

**What to do**
Add a receive deadline to FramedStream beside the max frame length, and make NodeFinder set it after establishment instead of wrapping each receive.
Stamp a Session when it enters the accept channel and discard it at `accept` after 10 seconds.
Give each use its own receiver lock in SessionAccepter, because an accept waiting on one use holds the lock that every other use needs; this surfaced when FileExchanger's accept task started next to NodeFinder's.

**Completion criterion**
A test shows a silent peer makes `recv_message` fail at the deadline, an unaccepted Session is discarded, an accept waiting on one use does not block another, and the existing NodeFinder tests pass.

<a id="s-2"></a>
### Step 2. Reject Unknown results and check the candidate's public key

**Locations to change**
`core/session/connector.rs` (result check), `core/negotiator/node/task_connector.rs` (public key check).

**What to do**
Succeed only on Accept.
After connecting, compare `session.cert`'s public key with the candidate NodeProfile's and, on mismatch, close the Session and try the next address.

**Completion criterion**
Tests show an Unknown-returning accepter fails the connection and a different key at the candidate's address is not recorded as connected.

<a id="s-3"></a>
### Step 3. Hash verification and provider entrances in the Store

**Locations to change**
`file_subscriber/store.rs`, `file_subscriber.rs`, `file_publisher/store.rs`, and the round-trip tests.

**What to do**
Return `WriteBlockResult` (stored, unneeded, mismatch) from `write_block` after hashing with the row's algorithm.
Add the provided root hashes (Downloading, Decoding, Finalizing, Completed), a block read limited to them, and the un-fetched hashes of the current rank.
Reject `import` over 1 MiB.

**Completion criterion**
Tests show a mismatched block leaves neither row nor body, a Canceled subscription's blocks are not readable for providing, and an oversized `import` is rejected.

<a id="s-4"></a>
### Step 4. Wire definition and frame limit

**Locations to change**
`daemon/rpfs/file.rpf`, `modules/engine/src/generated`, `base/connection/stream/framed.rs`, `core/session/model.rs`.

**What to do**
Define the version, root hash, accept and reject, status, request, block, and do-not-have messages as one enum with the constants of §5.8.
Regenerate the code and lower `FILE_EXCHANGER_MAX_FRAME_LENGTH` to 1 MiB plus the block overhead.

**Completion criterion**
Round-trip tests of every message pass, a block of exactly 1 MiB passes the frame limit, and a larger one is rejected.

<a id="s-5"></a>
### Step 5. Decision parts: assignment, choke, exclusion, Intervals

**Locations to change**
New files under `core/negotiator/file/` for `FileExchangerIntervals`, the assigner, the choke manager, and the exclusion list.

**What to do**
Implement the logic of §5.4 to §5.7 as types that take time as an argument and do no I/O.

**Completion criterion**
Unit tests cover single assignment, return on do-not-have, choke, close and mismatch, the re-request delay, the unchoke ranking with the random slot, and exclusion expiry.

<a id="s-6"></a>
### Step 6. Session registry, handshake, and per-Session worker

**Locations to change**
New files under `core/negotiator/file/`; remove `session_status.rs`.

**What to do**
Register one Session per node ID and root hash and keep the smaller node ID's on a duplicate.
Exchange versions and the root hash, then run the worker that sends the periodic status, requests, and blocks and applies the deadlines and violation rules.

**Completion criterion**
Loopback tests cover transfer between two nodes, relay by a node in the middle of fetching, each violation, choke, the request and receive deadlines, and the close of a mutually-unwanted Session.

<a id="s-7"></a>
### Step 7. Connector, accepter, and location finder

**Locations to change**
`task_connector.rs`, `task_accepter.rs`, `file_exchanger.rs`, a new location finder trait with a NodeFinder implementation.

**What to do**
Dial fetch and serve targets in separate slots with the per-root-hash limit, skip excluded and closed pairs, accept with the §5.2 checks, and advertise and want the provided root hashes through the trait.

**Completion criterion**
Tests with a fixed-location implementation show slot limits, rejection reasons, and candidate skipping.

<a id="s-8"></a>
### Step 8. AxusService wiring and the real-NodeFinder test

**Locations to change**
`service.rs`.

**What to do**
Add the FileExchanger use to the SessionAccepter, create FileExchanger, and stop it in order with a rollback on a failed start.

**Completion criterion**
A test with two nodes wired as AxusService wires them (real NodeFinder with short intervals) shows advertisement, discovery, and transfer, a test shows AxusService accepts the FileExchanger use, and a failed start leaves no running worker.

## 6. Acceptance Verification

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1, 2, 4, 5 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/file_exchanger::tests/)'` | All transfer, relay, violation, and deadline tests pass |
| 3, 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/file_subscriber\|file_publisher\|protocol::file\|framed/)'` | All pass |
| 7, 8 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/session::tests\|receive_deadline\|node::task_connector/)'` | All pass |
| 9 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/service::tests\|file_exchanger::tests/)'` | All pass |
| 10 | `cd daemon && RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo make test` | Exit code 0 |
