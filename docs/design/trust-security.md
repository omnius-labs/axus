# Design of trust and security

## 1. About this document

This document covers what the Session, Merkle hash, and Web of Trust each guarantee, and the boundaries of node identity and threats.
Term definitions are the source of [terms.md](../terms.md).

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | Scale of security guarantees, threats, identity, and the Web of Trust design decisions |
| [session.md](./session.md#2-responsibilities-and-boundaries) | Session establishment and use separation |
| [file-transfer.md](./file-transfer.md#5-block-exchange) | Content hash verification of blocks |
| [profile-memo.md](./profile-memo.md#2-responsibilities-and-boundaries) | Profile payload and memo responsibility boundary |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §5.

## 2. Responsibilities and boundaries

Authentication, encryption, content hashing, and trust evaluation are separate guarantees, and no single one replaces another.
This document holds the boundary of each guarantee and the identity and Web of Trust decisions derived from it.
The individual Session protocol, file protocol, and Profile payload structures belong to their corresponding design documents.

## 3. Division of guarantees and threats

| Mechanism | Guarantees | Does not guarantee |
| --- | --- | --- |
| Session secure channel | That the communication peer is the signer, and the confidentiality and tamper detection of post-establishment communication | Trust in the peer, correctness of the content the peer publishes |
| Public key reconciliation at the NodeFinder handshake | That the peer's NodeProfile and node ID belong to the Session's signer | That the reachable address still belongs to that node |
| NodeProfile signature | That the owner showed the public key, reachable addresses, and update time | Trust in the node, that the reachable address still belongs to that node |
| FramedStream | Message boundaries and the frame size limits | Confidentiality and tamper detection of the communication content |
| Merkle hash | That a block's content matches the expected hash | Reliability of the provider, soundness of search results |
| Web of Trust | The trust policy decided in §4 | Transport encryption and block content verification |

Even when Session authentication succeeds, the peer's NodeProfile and public content are not necessarily trustworthy.
Under this premise, identity, encryption, content verification, and trust evaluation can be implemented separately.

The node ID is derived from the public key, but because keys can be generated at low cost, an attacker can gather IDs close to an AssetKey in Sybil and Eclipse attacks.
The count limits on known nodes and propagated information curb resource consumption but do not prevent only the attacker's information from remaining.
Web of Trust takes on this trust selection, and wiretap and tampering of the transport are prevented by the Session secure channel ([session.md](./session.md#secure-channel-of-the-session)).

NodeFinder relay can amplify information, so TTL, count, connection count, and message-size limits and the handshake and receive deadlines are enforced at protocol input boundaries.
The handshake from Session authentication through use selection spans 10 seconds in total after the TCP connection, and the accepting side runs at most 64 concurrently.
Connections over the limit are closed without reading or writing, and connections past the deadline are also closed.
The frame limits are 16 KiB during the handshake, 256 KiB for NodeFinder after use selection, and a value based on FileExchanger's maximum block length of 1 MiB; a Session whose reception exceeds the limit is closed.
These are constraints for curbing the receiving side's resource consumption by pre- and post-authentication peers, and the details and rationale are the source of [session.md](./session.md#51-decided).
NodeFinder closes a Session that receives nothing, both during the Hello/Profile exchange and after establishment, for 3 times the communication cycle.
With the default communication cycle of 20 seconds the receive deadline is 60 seconds, and the peer sends a Status every cycle even when it has nothing else to send.
This send promise is the basis for limiting connection-resource occupation by an unresponsive peer, and the deadline is derived from the cycle setting ([node-finder.md](./node-finder.md#61-decided)).

At the wire decode of NodeMessage and NodeProfile, the following element counts are checked before allocating collections, and a Session over the limit is closed.

| Target | Limit per message |
| --- | --- |
| `AnnounceProfiles` NodeProfiles | 32 |
| `AnnounceLocations` AssetKeys | 1024 |
| `LookupResponse` locations | 8 |
| `LookupResponse` closer nodes | 8 |
| NodeProfiles per AssetKey | 8 |
| NodeProfile `addrs` | 8 |

The NodeProfile address count and byte-length checks also apply to the ProfileMessage and `axus:node/...` URIs.
When the local node has many addresses, they are truncated to 8 at startup before signing, with a warning logged.
The sending side randomly narrows the NodeProfile count within the same limits but does not change the reachable addresses of a signed NodeProfile.
Limiting both the frame byte count and the element count curbs the receiving side's allocations, processing, and the resource consumption of relaying location info.
The propagation procedure and the rationale for the count limits are the source of [node-finder.md](./node-finder.md#61-decided), and the byte lengths and send budget judgments are the source of [rocketpack.md](./rocketpack.md#5-design-decisions).

FileExchanger hash-verifies received blocks, NodeProfile verifies signatures, and the Profile exchange verifies signatures and versions.
FileExchanger excludes for a period the node ID that sent a block failing verification, but the exclusion does not reach a peer that regenerated its key (see [file-transfer.md](./file-transfer.md#57-verification-and-violations)).

## 4. Design decisions

### 4.1 Decided

The Session, block, and Profile individual decisions are the source of each design document.

#### node ID is derived from the public key

**Decision**
A NodeProfile carries the public key, not the node ID, and the node ID is derived as the SHA3-256 hash of the public key.
The signing key is stored in the state directory, and the same key and node ID are used after a restart.
NodeFinder's handshake rejects the connection when the public key of the peer's NodeProfile does not match the public key of the Session's cert.

**Rationale**
Deriving the node ID from the public key settles the signer's ID with only the Session signature verification, without separately proving the ID-to-key correspondence.
Because the ID is derived and not stored, a NodeProfile whose ID and key disagree cannot be expressed.
Rotating the key changes the node ID, but there is no requirement to carry the ID over at this point.

**Rejected**
A design that proves a stable ID-to-key correspondence with a signature can express key rotation, but requires a proof chain and revocation, so it is not taken.
A design that separates the ID and the key and ties them in a trust graph requires the Web of Trust design first and complicates every reconciliation, so it is not taken.

#### NodeProfile binds reachable addresses with the node's own signature

**Decision**
A NodeProfile consists of the public key, reachable addresses, update time, and a signature, and the signature is made with the owner's key.
The signed target is a fixed prefix indicating the use and version prepended to the bytes of the value without the signature encoded with RocketPack.
The signature is made once at startup, not on every connection.

The NodeProfile type can only be created through the path that signs with the local node's key, or the path that verifies the signature from the wire or a URI.
A signature mismatch and malformed format become a decode failure and close the Session.
Of the NodeProfiles received in an AnnounceProfiles message, one whose update time is more than 5 minutes ahead of the receive time is discarded on its own, keeping the Session.
The NodeProfile in the handshake ProfileMessage is not subject to the time check.
Refusal for being too old is not done.
Only signed URIs can be specified as configured bootstrap nodes.

For the same node ID, the NodeProfile with the larger update time is treated as newer, and known nodes keep only one entry per node ID (see [node-finder.md](./node-finder.md#known-nodes-keep-the-newest-nodeprofile-per-node-id)).

**Rationale**
Without a signature, a third party can attach fake reachable addresses to a node's public key and distribute them.
With a signature, a relaying node cannot rewrite the reachable addresses, and the receiver can verify, regardless of the distribution path, that the owner showed them.
The update time prevents redelivered old signed NodeProfiles from rolling the reachable addresses back.

Building the signed target by re-encoding keeps the wire shape flat and lets the public key and reachable addresses be read before signature verification.
Because RocketPack encoding is a pure function of the values, the signer and the verifier get the same bytes.
Since the same key also signs in the Session handshake, the prefix separates the uses so another signature cannot be used as a NodeProfile signature.

The 5-minute update tolerance is set equal to the clock difference the Session handshake allows, keeping the clock premise to one.
The Session is not closed over a NodeProfile past the tolerance because even an honest peer can send one depending on the clock difference between the relaying and receiving nodes.
The ProfileMessage is not checked because it is the peer's own NodeProfile and, after reconciliation with the Session cert's public key, is used only for identifying the Session and does not enter the known-node update order.
Refusing by age is not done because refusing would force long-running nodes to re-sign and re-advertise periodically, and URIs issued at startup for bootstrap would expire.
What redelivering an old signature causes is a connection attempt to an old reachable address, and even if that address now belongs to another node, the dialing side can distinguish it by reconciling the connection candidate's public key with the Session's cert.
The dialing sides of NodeFinder and FileExchanger perform this reconciliation, closing the Session and trying the next address when the key differs.

The signature shows only that the owner showed the reachable addresses.
It does not show trust in the node or that the reachable address still belongs to it.
A third party can provoke connection failures and push a specific node out of lookup.
This selection is taken on by the Web of Trust.
The cost is that a NodeProfile grows by the signature and update time.
Also, a NodeProfile re-signed with an update time no greater than the previous one due to a clock rollback is not used for replacement by the receiving side holding the previous value, and the owner must re-sign with a larger update time.

**Rejected**
Saving and distributing only the reachable addresses that connected successfully keeps the wire shape, but it leaves the stage of first trying the received addresses and cannot detect rewriting during distribution, so it is not taken.
Carrying `OmniCert` with the signer's name duplicates the public key on the NodeProfile and adds fields not used for signing, so it is not taken.
A design that signs the received bytes and carries an encoded body and signature on the wire avoids re-encoding, but extracting the public key requires decoding the body each time, and the shapes of the URI and the send budget calculation change, so it is not taken.
Defining the signed target with a concatenation rule separate from RocketPack is independent of encoding, but requires maintaining a second encoding convention, so it is not taken.
A monotonic counter for the update order does not depend on the clock, but needs persistence across restarts and handling of rollback when persistence fails, so it is not taken.
Refusing old NodeProfiles by age and re-signing periodically lets old information die naturally, but needs a periodic signing and advertising mechanism, so it is not taken.

### 4.2 Deferred

#### Unit and propagation of Web of Trust

**Current state**
The README advertises search and publish protection via Web of Trust, but the trust target and score calculation rules are not defined.

The candidates are the following two.

1. Using only direct signatures keeps explanation and revocation simple, but cannot evaluate unknown subjects.
2. Using transitive trust widens discovery, but requires defining depth, decay, cycles, and Sybil resistance.

**Why not decided yet**
Because the API that applies trust is not fixed.

**Decision condition**
Decide before defining the first API that selects search results or Profiles by trust score.

## 5. Current status and remaining work

Session verifies the peer's signature and key through the secure channel and encrypts post-establishment communication.
The signing key is stored in the state directory, the node ID is derived from the public key, and the NodeFinder handshake reconciles it.
The handshake deadline and concurrency and the frame limits during the handshake and after use selection are implemented.
The receive deadlines for NodeFinder's Hello/Profile exchange and after establishment are implemented as 3 times the communication cycle.
The wire decode of NodeMessage and NodeProfile checks the element count limits, and the sending side stays within the same limits.
NodeFinder's frame and NodeMessage are limited to 256 KiB or less, and the variable-length field constraints and the send candidate budget selection are implemented.
Tests confirm disconnection on each over-limit, acceptance exactly at limits, and round trips of large information.
NodeProfile is generated with a signature and verified through the paths built from the wire and a URI.
It is signed at startup with the local node's key, and of the NodeProfiles received in an AnnounceProfiles message, those whose update time is ahead by more than the tolerance are discarded.
Tests confirm signature verification, rejection of tampering, fixed signed bytes, and replacement by update time.
The Web of Trust policy is undecided.
The process in which the dialing side reconciles a connection candidate's public key with the Session's cert is listed as a known bug in [issues.md](../issues.md).
