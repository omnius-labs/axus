# NodeFinder design

## 1. About this document

This document covers the design of NodeFinder, which exchanges the location information of nodes and finds the nodes that advertise an AssetKey.
Term definitions are the source of [terms.md](../terms.md).

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | NodeFinder lookup, information propagation, distance calculation, persistence |
| [file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries) | Publishing and subscribing to files using lookup results |
| [node_finder.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs) | NodeFinder implementation |
| [node_finder_repo.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs) | Persistence of known nodes |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §7.

## 2. Responsibilities and boundaries

NodeFinder handles the exchange of known nodes and the location information of nodes that advertise an AssetKey.
A request is conveyed with only the AssetKey and does not carry the requester's NodeProfile, so NodeFinder returns only the nodes that advertise that AssetKey.
Which assets to advertise is decided by the higher-level component.
Because FileExchanger also advertises files being fetched, the location info of a file can include nodes that hold only some of the blocks (see [file-transfer.md](./file-transfer.md#subscriptions-are-advertised-as-locations-from-the-start)).
NodeFinder does not interpret the content of an asset and returns only the AssetKey-to-NodeProfile mapping.
FileExchanger uses this boundary, so lookup messages and file transfer messages are not mixed.

[NodeProfileFetcher](../../daemon/modules/engine/src/core/negotiator/node/node_profile_fetcher.rs) fetches a set of NodeProfiles for bootstrap from outside.
The URI conversion details and checksum are the source of [protocol/uri.rs](../../daemon/modules/engine/src/protocol/uri.rs).

## 3. Information distribution

NodeFinder sends one message per purpose: a periodic Status keeps a Session alive, an announcement carries the nodes that advertise an AssetKey toward the nodes closest to it, and a lookup asks those nodes directly.
A lookup is a request and an answer on the same Session, so its result does not wait for a later cycle.

```mermaid
flowchart LR
    Want[want_asset_keys]
    Give[give_asset_key_locations]
    Push[push_asset_key_locations]
    Nodes[push_node_profiles]
    Near[Nodes near the AssetKey]
    Asker[Requester node]
    All[Connected peers]
    Repo[(NodeFinderRepo)]

    Want --> Near
    Push --> Near
    Near --> Give
    Give --> Asker
    Nodes --> All
    All --> Repo
```

Announcements are edged toward nodes close in XOR distance and relayed with a decreasing hop count, while a lookup answer returns to the requester on the Session that carried the request.
Received information is given a lifetime and count limit so old location info and unbounded relay are not retained.

## 4. Separation of decision and communication

[TaskComputer](../../daemon/modules/engine/src/core/negotiator/node/task_computer.rs) looks at the receive state of all Sessions and the requests of higher-level components and computes the next DataMessage to send.
[TaskCommunicator](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs) handles only the per-Session handshake and send/receive.
Consolidating the decision into one place keeps routing policy from scattering across per-connection workers.
Each Session has an outbox and a writer task, so a message produced outside the cycle is sent without waiting for it, and the writer sends a Status when a cycle passes with nothing to send.
The wait comes after processing, and since the §6.1 receive deadline is updated at message reception, the wait does not extend the deadline.
A peer that sends faster than the cycle is prevented from making the receiving side repeat persistence work.

The boundary that receives the needed AssetKeys from FileExchanger uses [FnHub](../../daemon/modules/engine/src/base/sync/fn_hub.rs).
Separating the firing side from the registration side lets NodeFinder avoid referencing FileExchanger's concrete type.

## 5. Distance and persistence

[Kadex](../../daemon/modules/engine/src/core/negotiator/node/kadx.rs) compares the XOR distance between an AssetKey's hash and a NodeProfile's ID and chooses the closer nodes.
Because the byte order of the distance calculation affects protocol interoperability, a separate implementation must promote the rule in the code to an external specification.

[NodeFinderRepo](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs) persists known nodes in SQLite as URI strings.
Storing a URI as an opaque value avoids changing the table schema on every format addition, but address-level SQL search is not possible.

## 6. Design decisions

### 6.1 Decided

The separation of NodeFinder and FileExchanger is the source of [design.md](../design.md#51-decided).

#### Keep the Session established by the smaller node ID when duplicates occur

**Decision**
When a Session with the same peer is duplicated, the Session established by the side with the smaller node ID is kept.
If the Session to keep finishes its handshake later, the earlier-registered Session is closed and swapped out.

**Rationale**
TaskConnectors of the same node reserve their connecting peer, so a duplicate happens when two nodes connect to each other simultaneously.
A rule that keeps the Session that finished the handshake first can make both sides keep different Sessions.
Because one side closing the connection is also the Session kept by the other, both Sessions close, and the dialer stays with the peer recorded as connected and does not reconnect.
Since the node ID is derived from the public key verified in the Session's signature, both sides choose the same Session with the same rule without an extra message.

**Rejected**
A design where the side that detects the duplicate notifies the peer and agrees on which Session to keep needs new messages and state transitions on top of the Session, so it is not taken.

#### The local NodeProfile carries the addresses decided at startup

**Decision**
The local NodeProfile carries the addresses of the setting `p2p.advertise_addrs` when present.
Without the setting, it carries the listen address when listening on a specific address, and when listening on an unspecified address, the reachable local IPs and UPnP external IPs with the listen port appended.
UPnP is used only when enabled by the setting `p2p.use_upnp`.
The addresses are decided once at startup, and an IP change is reflected by restarting.
If more than 8, they are truncated to the first 8 with a warning logged, and startup continues.
The NodeProfile signature is made once, after the addresses are decided and truncated.

**Rationale**
Because the decision uses only what the node knows itself, no message needs to be added to the handshake.
When listening on `0.0.0.0` or behind a NAT without UPnP, a reachable address cannot be decided automatically, so the explicit setting is preferred.
Because UPnP changes the router's port-forwarding configuration, it runs only when chosen by the user.

**Rejected**
A design that has the connection target return the source address it saw through the handshake can learn the address outside the NAT, but it adds a handshake message and needs a way to verify the peer's report, so it is not taken.

#### NodeFinder receive deadline

**Decision**
NodeFinder closes a Session when it receives no message from the peer, including the Hello/Profile exchange, for 3 times the communication cycle.
The communication cycle is taken from `NodeFinderIntervals::communicate`; with the default of 20 seconds the receive deadline is 60 seconds.
The deadline is held by FramedStream ([session.md](./session.md#post-establishment-receive-deadline)), updated on each Hello, Profile, and DataMessage reception, and not extended by the local node's sends.
After establishment, a DataMessage is always sent every communication cycle, even when empty.
Shutdown ends the send/receive task with the cancellation token without waiting for the receive deadline, and waits for its termination.

**Rationale**
Since the peer sends a DataMessage every cycle, a 3-cycle grace period still prevents an unresponsive Session from permanently occupying connection resources.
Deriving the deadline from the communication cycle keeps the ratio of grace to send frequency even when the cycle setting changes.
Applying it to the Hello/Profile exchange also closes connections that finish Session authentication and use selection but then do not respond.

#### Element counts of DataMessage and NodeProfile

**Decision**
The element counts of DataMessages sent and received are kept within the following ranges.

| Target | Limit per message |
| --- | --- |
| `push_node_profiles` | 32 |
| `want_asset_keys` | 1024 |
| `give_asset_key_locations` | 1024 |
| `push_asset_key_locations` | 1024 |
| NodeProfiles per AssetKey | 8 |
| NodeProfile `addrs` | 8 |

The receiving side checks the limits right after reading the element count, before allocating a Vec or HashMap, and closes the Session of an over-limit message.
The NodeProfile address count is checked at wire decode, so it also applies to the ProfileMessage after Hello.
The `axus:node/...` URIs used for configuration and persistence also follow the same generated NodeProfile constraints and signature verification.
The sending side randomly narrows candidates with the existing rng and stays within each limit.
After the count narrowing, candidates that fit the byte budget of [rocketpack.md](./rocketpack.md#alternating-candidate-types-within-the-size-budget) are chosen.
The receive state holding multiple messages' worth has its own TTL and aggregation count limits.

**Rationale**
Not only the byte count of a frame but also the element counts and the spread of location info are limited, curbing the receiving side's allocations, processing, and the resources consumed by relay.
The 32 known nodes are the same value as the old post-reception truncation, and along with the other limits they are moved to the decode input boundary.
Keeping the sending side within the same limits avoids assembling messages the peer would reject.
Random selection when narrowing the count reduces the bias of always propagating the same candidates.

#### Known nodes keep the newest NodeProfile per node ID

**Decision**
NodeFinderRepo keeps one NodeProfile per node ID.
When a NodeProfile of the same node ID is received, it replaces the held value only when its update time is greater; an equal or smaller value is ignored.
Replacement does not change the row's weight.
The signature mechanism is the source of [trust-security.md](./trust-security.md#nodeprofile-binds-reachable-addresses-with-the-nodes-own-signature).

**Rationale**
Using the whole URI as a key leaves a separate row every time the same node's addresses change, increasing connection attempts to unreachable addresses.
With a signed NodeProfile, a larger update time can be verified as the owner's newer reachable addresses, so the node ID can be the key and only the newer value kept.
The weight is the priority attached to a NodeProfile's origin and does not change as reachable addresses update.

**Rejected**
Keeping the URI as the key and returning only the newest value per node ID at read time avoids changing the storage format, but the old rows keep consuming the count limit.

#### One message per purpose instead of one delta per cycle

**Decision**
The established phase carries `NodeMessage`: `Status`, `AnnounceProfiles`, `AnnounceLocations`, `LookupRequest`, and `LookupResponse`, and the handshake uses `Hello` and `Profile` of the same enum.
A want is not a message; it stays a local request that starts a lookup.

**Rationale**
The delta carried requests, answers, and advertisements in one frame, so a want was re-relayed for the whole lifetime of the receive state and an advertisement had no bound on how far it travelled.
Separating the purposes removes the relay of requests and gives each remaining message its own limit.

**Rejected**
Keeping the delta and adding a lookup message would keep two distribution models in one Session.

#### Relay an announcement with a decreasing hop count

**Decision**
An announcement starts with `MAX_HOPS` at the node that provides the AssetKey.
A node that receives one stores the locations and, while the remaining hop count is above zero, relays it once to the three connected nodes closest to the AssetKey with one hop less.
A receiver rejects a hop count above `MAX_HOPS`.
`AnnounceProfiles` goes to every connected Session without relaying.

**Rationale**
A bound on the number of relays keeps an advertisement from circulating for as long as its lifetime allows, and the relay is what lets an announcement reach the nodes closest to a key when the provider is not connected to them.
The known nodes are small and bounded, so profile gossip keeps going to every Session.

#### Keep the received locations in one node-level cache

**Decision**
One `LocationCache` holds the locations of every AssetKey, fed by both an answer and an announcement, with a lifetime of 30 minutes refreshed on receipt.
It holds at most 4,096 AssetKeys and 16,384 NodeProfiles and evicts the entry seen longest ago first.
`find_node_profile` reads this cache.

**Rationale**
Locations lived in the Session that carried them, so they disappeared when that Session ended, and an advertisement that arrived as a push was never used for a local lookup.
One cache also gives the receive state a single bound instead of one per Session.

#### Find an AssetKey by an iterative lookup

**Decision**
The lookup task asks the three known nodes closest to the AssetKey, collects the locations and the closer nodes of each answer, and continues with the closer ones until an answer carries a location or the deadline passes.
A node that is not connected is dialed and the Session is registered as usual.
The same AssetKey is not looked up again for three lookup intervals.

**Rationale**
Without a request and an answer, a lookup could not tell "nobody has this key" from "it has not arrived yet", and the caller had to poll the propagation state.
Iterating toward closer nodes reaches a provider through nodes the asker is not connected to.

#### Rank known nodes by when they were last reached

**Decision**
The store keeps `last_seen`, `fail_count`, and `last_attempt` per node instead of `weight`.
A finished handshake marks the node reachable and clears the failure count; a dial that fails on every address marks it unreachable.
The wait before the next dial doubles per failure up to 30 minutes.
The oldest `last_seen` is evicted first.

**Rationale**
The row update time moved only when a newer signed NodeProfile arrived, so a node whose addresses never change was evicted before one that kept bumping its timestamp.
A received profile does not prove the node is reachable, so gossip must not count as an observation.

### 6.2 Deferred

#### Reachability and latency at real operating scale

**Current state**
The lookup runs with a fixed fan-out of three and a deadline of three communication cycles.
The routing table is the verified profile store, without k-buckets.

**Why not decided yet**
The reachability and latency of a multi-hop lookup at real operating scale have not been measured.

**Decision condition**
Decide the fan-out, the deadline, and whether to keep a separate routing table once a multi-hop integration test measures them.

## 7. Current status and remaining work

The connection, accept, computation, communication, and lookup tasks and the SQLite repo exist.
Every Session carries `NodeMessage`, and an idle Session stays open through the periodic Status while a silent peer closes after three communication cycles.
Announcements are relayed with a decreasing hop count, and their locations live in one node-level cache with the limits of §6.1.
The lookup asks the closest known nodes, dials when needed, and answers from the cache and the routing table.
`cargo make lint` and `cargo make test` pass, except for the `omnius-core-migration` PostgreSQL tests that need a database in this environment.

The reachability and latency of a multi-hop lookup at real operating scale are to be measured, and the fan-out, the deadline, and the routing table are to be decided from them.

For confirmed bugs, see [issues.md](../issues.md).
