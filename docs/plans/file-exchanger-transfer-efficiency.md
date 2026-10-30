# Plan: FileExchanger transfer efficiency

## 1. About This Plan

This plan implements the fetch-side refinements of [file-transfer.md §5](../design/file-transfer.md#5-block-exchange).
It replaces the per-hash answer with one presence bitmap per request, adds tail duplication with a cancel message, retracts a request that stays unanswered, and orders the dialing candidates by the measured throughput.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| The protocol, constants, and the reasons for the decisions | [file-transfer.md §5](../design/file-transfer.md#5-block-exchange) and [§6.1](../design/file-transfer.md#61-decided) |
| Vocabulary | [terms.md](../terms.md#2-terminology) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-10-29 against commit `19bf5b9`.

## 2. Purpose and Completion Criteria

A fetch no longer waits on one slow peer at its tail, a peer that holds nothing of a rank stops receiving requests for that rank, and a peer that answers one block a minute stops holding its assignments.
The starting point is the four costs recorded in §5 of the design: the absent hashes of a peer cost a message each, a slow peer decides the tail, the request deadline lets a slow trickle keep its blocks, and the fetch order is random.

### 2.1 Completion Criteria

1. The answer to a request starts with one presence bitmap, the absent hashes are released before the first block arrives, and the node is not asked for that rank again for the period.
2. The same block is assigned to two Sessions only in the tail, the first block stored completes it, and the other Session is sent a cancel.
3. A block for a retracted hash that arrives within the period is dropped without excluding the node.
4. A request that stays unanswered past the soft deadline is retracted while the Session stays open, and its blocks complete from another Session.
5. The dialing candidates are ordered by the measured throughput, and a slow node keeps fewer requests awaiting responses.
6. A presence bitmap whose number of bits differs from the request is a violation, and the violations of §5.7 still hold.
7. `cargo make lint` and `cargo make test` pass.

### 2.2 Non-goals

| Item | Where it goes |
| --- | --- |
| Send bandwidth limit | [file-transfer.md §6.2](../design/file-transfer.md#62-deferred) |
| Bloom filter of the provided hashes | Rejected in [file-transfer.md §6.1](../design/file-transfer.md#convey-the-presence-of-a-whole-request-in-one-bitmap) |
| Persisting the throughput and the exclusion | Rejected in [file-transfer.md §6.1](../design/file-transfer.md#record-transfer-volume-per-node-id-and-narrow-send-targets-with-choke) |
| REST API for publish and subscribe | Roadmap item 1 in [design.md](../design.md#62-roadmap) |

## 3. Current State

[file.rpf](../../daemon/rpfs/file.rpf) defines `Request`, `Block`, `NotHave`, and `Status`, and the peer answers each requested hash with a block or a separate `NotHave`.
[block_assigner.rs](../../daemon/modules/engine/src/core/negotiator/file/block_assigner.rs) keeps `assigned: HashMap<OmniHash, u64>` and `not_have` as expirations of `(node ID, hash)`, so one block belongs to one Session and every absent hash has an entry.
[session_worker.rs](../../daemon/modules/engine/src/core/negotiator/file/session_worker.rs) tracks `pending` as the hashes of each request and measures the request deadline from the last answer of any kind.
[task_connector.rs](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs) shuffles the locations of a root hash and dials them in that order, and [file_subscriber/repo.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/repo.rs) returns the un-fetched hashes in rank and index order without the rank.

Without these changes, the block exchange of §5 works, but a request answered by a peer holding nothing costs a message per hash, the tail waits on one peer, and a slow trickle keeps its assignments.

## 4. Change Approach

The wire change comes first, because the tail duplication and the retraction are both expressed with the cancel message.
The presence bitmap and the rank throttle replace the per-hash record of the assigner, then the assignment changes for the tail, the throughput feeds the dialing order and the pending count, and the worker gains the soft deadline that retracts a request on top of both.

### 4.1 Assumed Decisions

| Decision | Source |
| --- | --- |
| Presence bitmap per request and the rank throttle | [file-transfer.md §6.1](../design/file-transfer.md#convey-the-presence-of-a-whole-request-in-one-bitmap) |
| Two copies in the tail and the thresholds | [file-transfer.md §6.1](../design/file-transfer.md#assign-one-block-to-two-sessions-in-the-tail) |
| Cancel and the block that crosses it | [file-transfer.md §6.1](../design/file-transfer.md#retract-blocks-with-a-cancel-message-and-drop-a-block-that-crosses-it) |
| Per-request retraction instead of closing the Session | [file-transfer.md §6.1](../design/file-transfer.md#judge-a-slow-peer-per-request-and-keep-the-session) |
| Fetch throughput in the dialing order and the pending count | [file-transfer.md §6.1](../design/file-transfer.md#order-fetch-targets-by-the-measured-throughput) |
| Time constants injected through Intervals | [file-transfer.md](../design/file-transfer.md#inject-fileexchangers-time-constants-through-intervals) |

### 4.2 Scope of Impact

Touched: `daemon/rpfs/file.rpf` and its generated code, plus `core/negotiator/file` and `protocol/file.rs`.
The wire changes in place under the V1 version bit; both ends of a Session run the same version, because the FileExchanger wire has no deployment that would need the old messages.
The NodeFinder wire and the data plane of the Store are unaffected; the publisher and subscriber Stores only gain the existence check used to build the bitmap.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [1](#s-1) | Presence bitmap and rank throttle | | done |
| [2](#s-2) | Cancel message and tail duplication | 1 | done |
| [3](#s-3) | Fetch throughput in the dialing order and the pending count | 1 | done |
| [4](#s-4) | Per-request retraction | 2, 3 | todo |

<a id="s-1"></a>
### Step 1. Presence bitmap and rank throttle

**Locations to change**
`daemon/rpfs/file.rpf` and the generated code (remove `NotHave`, add `HaveMap`), `protocol/file.rs`, `session_worker.rs`, `block_assigner.rs`, `exchange_context.rs`, `file_subscriber/repo.rs`, `file_subscriber/store.rs`, `file_publisher/store.rs`, `file_exchanger_intervals.rs`.

**What to do**
Define the bitmap as one bit per requested hash in the request order, and send it before the blocks that answer the same request.
Build it from an existence check on both Stores instead of reading each block.
Replace the per-hash `not_have` with an expiration per node ID, root hash, and rank, and carry the rank of each un-fetched hash from the Store to the assigner.
Treat a bitmap whose number of bits differs from the request as a violation.

**Completion criterion**
Loopback tests show that a request to a peer holding nothing releases every hash from the bitmap alone, that the same peer is not asked again for that rank until the period passes, and that a mixed answer stores the blocks it holds without throttling the rank.
A test shows the bitmap of 64 hashes is 8 bytes and a block of 1 MiB still fits the frame limit.

<a id="s-2"></a>
### Step 2. Cancel message and tail duplication

**Locations to change**
`daemon/rpfs/file.rpf` and the generated code (add `Cancel`), `protocol/file.rs`, `session_worker.rs`, `serve_queue.rs`, `block_assigner.rs`, `exchange_context.rs`, `file_exchanger_intervals.rs`.

**What to do**
Add the cancel of a list of hashes, and remove those hashes from the `ServeQueue` of the answering side.
Keep a short expiration of the hashes a Session retracted, and drop a block whose hash is in it without treating the block as a violation.
Allow the assigner to hold two Sessions for one hash, and start that when the un-fetched blocks fall to the tail thresholds.
Cancel the other copies when one copy is stored.

**Completion criterion**
A test shows that a block is assigned to two Sessions only after the threshold, that the first stored copy completes it and the other Session receives a cancel, and that a block sent before the cancel arrives is dropped without excluding the node.
A test shows a block that was never requested and never cancelled is still a violation.

<a id="s-3"></a>
### Step 3. Fetch throughput in the dialing order and the pending count

**Locations to change**
`choke_manager.rs` or a new record beside it, `exchange_context.rs`, `task_connector.rs`, `session_worker.rs`, `file_exchanger_intervals.rs`.

**What to do**
Record the throughput of the blocks received with a matching content hash per node ID, over the period of §5.8, and use it in two places: order the dialing candidates of a root hash, and set the number of requests awaiting responses on a Session between the minimum and the maximum.
A node with no record is tried at a random position, and a node counts only after the minimum number of blocks, so one fast block does not decide the order.

**Completion criterion**
Tests with fixed locations and a slow peer show that the fast peer is dialed first and keeps the maximum of pending requests, that a node without a record is tried, that a node below the minimum number of blocks is not scored, and that the record decays after the period.
A test shows a slow node is not excluded for being slow.

<a id="s-4"></a>
### Step 4. Per-request retraction

**Locations to change**
`session_worker.rs`, `block_assigner.rs`, `exchange_context.rs`, `choke_manager.rs`, `file_exchanger_intervals.rs`.

**What to do**
Record when each request was sent, and retract it when it stays unanswered past the soft deadline while the Session answers other requests: send a cancel and release its hashes to the assigner.
Keep the request deadline as the close condition, and count a presence bitmap as an answer.

**Completion criterion**
A test shows a peer that answers one block every cycle keeps the Session but loses the blocks of its unanswered request, which complete from another peer.
A test shows a peer that answers nothing is still closed and excluded at the request deadline.

## 6. Acceptance Verification

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1, 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/protocol::file\|file_exchanger::tests/)'` | All pass |
| 2, 3 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/file_exchanger::tests\|block_assigner/)'` | All pass |
| 4, 5 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/file_exchanger::tests\|choke_manager/)'` | All pass |
| 7 | `cd daemon && RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo make test` | Exit code 0 |
