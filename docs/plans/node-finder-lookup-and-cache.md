# Plan: NodeFinder location cache, active lookup, and protocol revision

## 1. About This Plan

This plan replaces NodeFinder's periodic delta message with a message-per-purpose protocol, moves the received location information into one node-level cache, makes the XOR distance carry a hop limit, and adds an iterative lookup.
It also makes the known-node store rank nodes by when they were last observed and backs off dialing a node that keeps failing.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| Lookup, propagation, distance, and persistence, and the reasons for the decisions | [node-finder.md](../design/node-finder.md) |
| Vocabulary | [terms.md](../terms.md) |
| Wire format and migration | [rocketpack.md](../design/rocketpack.md) |
| Receive deadline and Session lifecycle | [session.md](../design/session.md) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-10-29 against commit `8dd1377`.

## 2. Purpose and Completion Criteria

Location information survives the Session that carried it, a node finds the nodes holding an AssetKey by asking, and the traffic that carries locations and requests is bounded by distance and by hop count.

### 2.1 Completion Criteria

1. A three-node chain shows a publisher's announcement relayed two hops, and a subscriber's iterative lookup finds the publisher and connects to it.
2. A lookup at a node that does not hold the AssetKey returns the nodes closer to it, the query advances to them, and the whole lookup ends within the deadline.
3. An entry whose remaining hop count is zero is not relayed, so one announcement does not circulate beyond the hop limit.
4. A want for an unpublished AssetKey does not dial repeatedly, which the dial count shows.
5. An idle Session stays established through the periodic Status, and a silent peer closes after three communication cycles.
6. Entries beyond the cache limits are dropped oldest `last_seen` first.
7. A node that keeps failing is dialed less often through its failure count, and the `weight` column no longer exists.
8. `cargo make lint` and `cargo make test` pass.

### 2.2 Non-goals

| Item | Where it goes |
| --- | --- |
| Changing FileExchanger's protocol or its `LocationFinder` API | Not needed: lookup results arrive in the cache that `find` already reads |
| REST API for publish and subscribe | Roadmap item 2 in [design.md](../design.md#62-roadmap) |
| Persisting the routing table, the location cache, or the exclusions | Rejected in [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| Compatibility with the previous `DataMessage` wire | The wire is replaced; see §4.1 |

## 3. Current State

NodeFinder exchanges one `DataMessage` per communication cycle, carrying `push_node_profiles`, `want_asset_keys`, `give_asset_key_locations`, and `push_asset_key_locations` ([node.rpf](../../daemon/rpfs/node.rpf)).
A received want is relayed again on every cycle for the 30-minute lifetime of the receive state, and an advertisement relays without a bound on how far it travels ([task_computer.rs](../../daemon/modules/engine/src/core/negotiator/node/task_computer.rs)).
Received information is held per Session, so it disappears when that Session ends, and `find_node_profile` scans only `give_asset_key_locations`, so an advertisement that arrived as a push is never used for a local lookup ([node_finder.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs)).
The receive state is capped at 262,144 entries per Session, which does not bound memory in practice ([task_communicator.rs](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs)).
`NodeFinderRepo` evicts by the row update time, which changes only when a newer signed NodeProfile arrives, so a node whose addresses never change is evicted before one that keeps bumping its timestamp ([node_finder_repo.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs)).
The `weight` column is read in the ordering and then discarded by a shuffle, and every write passes zero.
A connection attempt that fails is not recorded, so an unreachable node is drawn again every connect cycle ([task_connector.rs](../../daemon/modules/engine/src/core/negotiator/node/task_connector.rs)).
There is no request/response path at all, so a lookup cannot ask a peer, and the only way information arrives is propagation.

## 4. Change Approach

The cache moves to the node level first while the wire stays as it is, so the state change is verified on its own.
The protocol is then replaced with a message-per-purpose enum, which removes want propagation, adds hop-limited announcements, and keeps the Session alive with a periodic Status.
Persistence and connection learning follow, because the routing table that the lookup needs is built from the same verified profiles.
The iterative lookup is added last, on top of the cache, the routing table, and the new message types.

### 4.1 Assumed Decisions

| Decision | Source |
| --- | --- |
| Message-per-purpose protocol instead of one delta message per cycle | [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| Announcements relay with a hop limit; wants do not travel on the wire | [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| Iterative lookup with a deadline and closer nodes | [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| Known nodes ranked by `last_seen`, with a failure count for backoff | [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| Cache limits of 4,096 AssetKeys and 16,384 NodeProfiles | [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| No wire compatibility with `DataMessage`; `NodeFinderVersion::V1` stays as it is | This plan |

### 4.2 Scope of Impact

Touched: `daemon/rpfs/node.rpf` and its generated code, `protocol/node.rs`, `core/negotiator/node` (all tasks, the message types, the cache, the routing table, and the lookup), `core/negotiator/node/node_finder_repo.rs`, and `core/negotiator/file/location_finder.rs` only if a signature changes.
Affected without edits: FileExchanger, which keeps calling `find` and receives lookup results through the cache.
The NodeFinder wire becomes incompatible with the previous daemon in both directions; the daemon is a pre-release and no compatibility path is kept.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [0](#s-0) | Frame length per Session type | | done |
| [1](#s-1) | Location cache and receive limits | 0 | done |
| [2](#s-2) | Message-per-purpose protocol, announcements, and Status | 1 | done |
| [3](#s-3) | Persistence by last seen time and dial backoff | | done |
| [4](#s-4) | Routing table and active lookup | 2, 3 | done |
| [5](#s-5) | Distance order, cleanup, and document close-out | 4 | done |

<a id="s-0"></a>
### Step 0. Frame length per Session type

**Locations to change**
`base/connection/stream/framed.rs`, `core/session/model.rs`, `protocol/node.rs`, `protocol/file.rs`, and the tests that build a `FramedStream`.

**What to do**
Take the maximum frame length as an argument of `FramedStream::new` and move the two constants into the protocol modules, so the Session layer reads the protocol constants instead of the stream owning them.

**Completion criterion**
`cargo make lint` and `cargo make test` pass with the constants in the protocol modules.

<a id="s-1"></a>
### Step 1. Location cache and receive limits

**Locations to change**
New `core/negotiator/node/location_cache.rs` and `want_cache.rs`, `core/negotiator/node/node_finder.rs`, `task_computer.rs`, `task_communicator.rs`, and `session_status.rs`.

**What to do**
Hold the received locations in one cache for the node, fed by both the answer and the advertisement categories, keyed by AssetKey with a per-entry lifetime and `last_seen`.
Make `find_node_profile` read that cache.
Hold the wants the higher layer asks for in one cache as well.
Replace the 262,144-entry per-Session cap with 4,096 AssetKeys and 16,384 NodeProfiles for the whole node, evicting the oldest `last_seen` first.

**Completion criterion**
A test shows an advertisement received as a push is returned by `find_node_profile`, the information survives the Session that carried it, and an over-limit insert evicts the oldest entry.

<a id="s-2"></a>
### Step 2. Message-per-purpose protocol, announcements, and Status

**Locations to change**
`daemon/rpfs/node.rpf` and its generated code, `protocol/node.rs`, `core/negotiator/node/message.rs`, `task_communicator.rs`, `task_computer.rs`, and `session_status.rs`.

**What to do**
Replace `DataMessage` with an enum carrying `Hello`, `Profile`, `Status`, `AnnounceProfiles`, and `AnnounceLocations`. Step 4 adds the lookup variants to the same enum.
Remove the want and answer fields from the wire; a want stays local.
Send the location announcements to the three connected nodes closest to the AssetKey, and relay a received announcement with one hop less until the hop count reaches zero.
Send the profile announcements to every connected Session without relaying.
Send a Status every communication cycle, and keep the three-cycle receive deadline.
Give each Session a send queue and a writer task, so a task can send outside the cycle, and dispatch a response to the pending request it belongs to.

**Completion criterion**
Tests show an announcement relayed exactly as many times as the hop limit allows, a zero-hop entry not relayed, a profile announcement not relayed, an idle Session kept open by Status, and a silent peer closed after three cycles.
`task_communicator::tests::a_relayed_announcement_carries_one_hop_less_to_the_other_session` observes the relayed hop count on the other Session, and `a_profile_announcement_is_not_relayed` observes that a profile announcement reaches no other Session.

<a id="s-3"></a>
### Step 3. Persistence by last seen time and dial backoff

**Locations to change**
`node_finder_repo.rs`, `task_communicator.rs`, `task_computer.rs`, and `task_connector.rs`.

**What to do**
Migrate the table to drop `weight` and add `last_seen`, `fail_count`, and `last_attempt`, with existing rows taking `last_seen` from the row update time.
Evict by `last_seen`, and update it only when a handshake with the node finishes, because a received profile does not prove that the node is reachable.
Back off dialing from `last_attempt` and `fail_count`, doubling the wait per failure, and reset both on a successful handshake.
Cache the verified profiles in memory instead of parsing and verifying every stored URI on each cycle, and run the shrink from the periodic task rather than from the receive path.

**Completion criterion**
Tests show the eviction order follows `last_seen`, an unreachable node is dialed with a growing interval, a success resets the count, and the receive path no longer runs a count query.

<a id="s-4"></a>
### Step 4. Routing table and active lookup

**Locations to change**
New `core/negotiator/node/routing_table.rs` and `task_lookup.rs`, `node_finder.rs`, `node_finder_repo.rs`, and `core/negotiator/file/location_finder.rs` if a signature changes.

**What to do**
Keep the verified profiles in distance order so a lookup can take the closest nodes to an AssetKey.
Send `LookupRequest` to three nodes at a time, take `LookupResponse.locations` into the cache, and continue with `closer_nodes` until no node is closer or the deadline passes.
Dial a node that the lookup needs when no Session exists, and register the established Session as usual.
Keep a per-AssetKey cooldown and a negative cache so a missing AssetKey does not dial on every cycle.
Raise the Session limits to eight connected and eight accepted.

**What it became**
The lookup runs as its own task over the local wants, every `NodeFinderIntervals::lookup` cycle with a cooldown of three cycles per AssetKey.
The whole lookup is cut at the deadline, dialing included, and it stops when no returned node is closer than the closest one already asked.
A provider keeps its own profile as the location of the AssetKeys it provides, so a lookup that reaches it is answered directly.
The XOR order used to choose the fan-out is `RoutingTable::closest_ids`; `Kadex::find` padded its result with the node itself and stopped at it, which dropped every candidate when the asking node was the closest one, so only `Kadex::compare` remains in use.

**Completion criterion**
The service tests reach a publisher through a bootstrap node and through a relay node without the propagation that Step 2 removed.
`task_lookup::tests::the_same_key_is_not_dialed_again_inside_the_cooldown` and `a_lookup_ends_within_its_deadline_even_when_dialing_hangs` cover the cooldown and the deadline, and the registry and routing-table unit tests pass.

<a id="s-5"></a>
### Step 5. Distance order, cleanup, and document close-out

**Locations to change**
`kadx.rs`, `task_connector.rs`, `node_finder.rs`, [node-finder.md](../design/node-finder.md), and [terms.md](../terms.md).

**What to do**
Promote the byte order of the XOR comparison into a stated rule with a test, so another implementation can reproduce it.
Lower the log level of an empty candidate list, and state how the connected and accepted limits are counted.
Record every decision of this plan in [node-finder.md §6.1](../design/node-finder.md#61-decided), move active lookup out of §6.2, and update §7 to the implemented state.

**Completion criterion**
The distance order has a test that fixes the rule, the document describes the implemented design, and no deferred item remains that this plan implemented.

## 6. Acceptance Verification

### 6.1 Test baseline

Four tests of `task_communicator` already fail on `262ac14` and on `8dd1377`: `push_node_profile_count_limit_closes_the_session_on_overflow`, `give_asset_key_location_count_limit_closes_the_session_on_overflow`, `push_asset_key_location_count_limit_closes_the_session_on_overflow`, and `large_computed_messages_roundtrip_without_closing_sessions`.
The first three wait 300 ms for a Session to close after an over-limit frame, but the receive loop reads that frame only after it finishes the rest of the communication cycle, so the close is measured at about 490 ms.
The fourth waits for the peer's receive state to reach the computed counts and times out.
All four are built on `DataMessage`, which Step 2 replaces, so the criterion for Steps 1 to 4 is that no failure other than these four is added, and Step 2 replaces the tests and closes them.
Step 2 removes the want and answer traffic, so `service::tests::node_finds_asset_key_owner_through_bootstrap_node` and `service::tests::node_finds_and_dials_an_owner_known_only_through_another_node` fail until Step 4 adds the lookup that replaces propagation.

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1, 2, 3 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/node::task_lookup\|node::task_computer\|node::task_communicator/)'` | All announcement, relay, and lookup tests pass |
| 4, 7 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/node::task_connector\|node::node_finder_repo/)'` | All backoff, eviction, and migration tests pass |
| 5, 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/node::location_cache\|node::task_communicator/)'` | All cache and deadline tests pass |
| 8 | `cd daemon && RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo make test` | Exit code 0 |
