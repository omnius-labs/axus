# Design of ProfileExchanger

## 1. About this document

This document covers how a node finds a Profile by its ID and exchanges the signed body with a peer.
The body and its lists belong to [profile.md](./profile.md#1-about-this-document), and the evaluation of the lists to [web-of-trust.md](./web-of-trust.md#1-about-this-document).
This document assumes the vocabulary of [terms.md](../terms.md). Term definitions are not given here.

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | Lookup, exchange, serving, and refresh of Profiles |
| [profile.md](./profile.md#1-about-this-document) | Profile identity, the signed body, and the update rules |
| [web-of-trust.md](./web-of-trust.md#1-about-this-document) | Why a Profile is fetched and what its lists mean |
| [node-finder.md](./node-finder.md#1-about-this-document) | Lookup of the nodes that advertise an AssetKey |
| [session.md](./session.md#2-responsibilities-and-boundaries) | Session establishment and the use selection |
| [trust-security.md](./trust-security.md#3-division-of-guarantees-and-threats) | What the Session authenticates and the frame layer |
| [file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries) | Publishing and subscribing to the files a FileRef names |
| [terms.md](../terms.md) | Term definitions |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §8.
To find out what runs now, read §8 first.

## 2. Responsibilities and boundaries

The ProfileExchanger turns a Profile ID into a signed Profile body.
It asks NodeFinder where the Profile is, dials the node, and exchanges the body over a Session.

It does not interpret the body.
The trust and block lists are opaque to the exchanger, and the evaluation belongs to [web-of-trust.md](./web-of-trust.md#4-evaluation).
It also does not publish files or carry blocks; a message or a file that is too large to sit in the Profile is a FileRef and is fetched by FileExchanger ([file-transfer.md](./file-transfer.md#5-block-exchange)).

The exchanger serves only the Profiles its node holds.
It never fetches a Profile on behalf of a requester, so a request cannot make the node spend another node's bandwidth.

## 3. Lookup

### 3.1 The AssetKey

A Profile is looked up by an AssetKey whose type is `profile` and whose hash is the SHA3-256 hash of the Profile ID string.
The ID itself travels in the exchange request, and the receiver checks that the body's ID and key produce the requested ID ([profile.md](./profile.md#31-profile-id)).

Hashing the ID keeps one key shape for every asset, and it lets a Profile keep its readable ID form in every user-facing place.
The hash is not the identity; the ID is.

### 3.2 Locations

The lookup returns the NodeProfiles of the nodes that advertise the key.
A node advertises the IDs of the Profiles it holds, in the same way FileExchanger advertises the root hashes it provides ([file-transfer.md](./file-transfer.md#51-participating-nodes-and-advertisement)).

In this version the held Profiles are the ones the node owns ([profile.md](./profile.md#33-several-profiles-on-one-node)).
A node that has fetched a Profile keeps it for evaluation but does not advertise it.

## 4. Session and exchange

### 4.1 The exchange use

The exchange uses a Session use of its own, beside the NodeFinder and FileExchanger uses.
The accepting side adds the use to its accepter, and the dialing side selects it after the handshake.

A separate use keeps the frame limit, the deadlines, and the failure rules of the Profile exchange out of the file transfer and the lookup.
The Session authenticates the peer and encrypts the exchange ([trust-security.md](./trust-security.md#3-division-of-guarantees-and-threats)), and the Profile signature decides whether the body is authentic ([profile.md](./profile.md#41-fields)).

### 4.2 Frame and messages

A frame is at most 1 MiB, and one response carries one Profile.
The message set is a request for an ID, a response with the signed Profile, and a not-found answer.

The receive deadline is 30 seconds, the same as FileExchanger's, and a receive that exceeds the frame limit or the deadline closes the Session.
The limit matches the maximum encoded Profile ([profile.md](./profile.md#7-limits)), so a valid Profile always fits one frame and a larger body is rejected as a violation rather than streamed.
A chunked transfer would allow larger Profiles, but it would also allow a peer to make a receiver allocate an unbounded body and would add a second consistency problem beside the update rules.

### 4.3 Serving rules

A node answers a request only when it owns the Profile.
A request for another Profile is answered not-found, and the dialer moves to the next location.

The node answers with the newest snapshot it owns.
It does not answer with an older snapshot on request; the update rules decide which snapshot is current ([profile.md](./profile.md#5-update-rules)).

A received body is verified before it is stored.
A body that fails the signature check or the update order is discarded and the request fails; the serving node is not excluded, because a bad body is not a transport violation ([profile.md](./profile.md#5-update-rules)).

## 5. Refresh

### 5.1 When a Profile is fetched

A Profile is fetched when the node needs it: at the first evaluation that reaches it, when the active Profile changes, and when a trust or block list adds an ID.
The fetched body is stored with its fetch time.

The followed set is the node's own Profiles, the trust and block entries of the active Profile, and the Profiles discovered at distance 2 during the evaluation.
The set is bounded by the evaluation limits ([web-of-trust.md](./web-of-trust.md#42-distance-and-the-expansion)).

### 5.2 Freshness and failure

A followed Profile is fetched again after a refresh period of 1 hour, and a manual refresh can fetch it earlier.
This version has no push: a node learns of an update only by fetching again.

A failed fetch leaves the last stored snapshot in place and retries with a backoff.
A Profile that was never fetched stays unknown ([web-of-trust.md](./web-of-trust.md#43-states)).
A not-found answer is a failure for that location, not a deletion of the Profile.

## 6. Limits and threats

| Item | Limit |
| --- | --- |
| Frame | 1 MiB |
| Receive deadline | 30 seconds |
| Profiles followed | 256 |
| Concurrent exchange Sessions | 8 |
| Refresh period | 1 hour |

A serving node can withhold a Profile, and the design accepts that: availability is not a guarantee of the exchanger.
It cannot forge a body, because the signature check fails, and it cannot roll a body back, because the receiver keeps the newer snapshot ([profile.md](./profile.md#52-replacement-and-rollback)).

A requester can make a node read and send a Profile, so the frame and the concurrent Session limits bound the work.
The bounds are per Session and per node, and the handshake deadlines of [session.md](./session.md#51-decided) bound the cost of a Session that never completes.

## 7. Design decisions

### 7.1 Decided

#### A Profile is looked up by the hashed ID under its own AssetKey type

**Decision**
The key is `profile` with the SHA3-256 hash of the ID string, and the ID travels in the request.

**Rationale**
One key shape serves every asset, and the readable ID stays the identity in every user-facing place.
The receiver checks the ID against the body, so the hash cannot be used to smuggle another Profile.

#### The exchange uses a dedicated Session use

**Decision**
ProfileExchanger gets its own use beside NodeFinder and FileExchanger.

**Rationale**
The frame limit, deadlines, and failure rules differ from both other uses, and mixing them in one use would make the limits the loosest of the three.

#### One Profile per 1 MiB frame

**Decision**
A Profile is limited to 1 MiB and a response carries one Profile.

**Rationale**
A valid Profile always fits one frame, and a chunked transfer would add an allocation bound and a consistency problem without a current need.

**Rejected**
Chunked transfer was considered and rejected because the Profile limits keep a body inside one frame, and the message and FileRef lists have their own bounds.

#### Only the owner serves a Profile

**Decision**
A node advertises and serves the Profiles it owns, and a fetched Profile is not served again.

**Rationale**
Serving a cached Profile would spread stale snapshots and make a node a party to a statement it did not make.
Availability comes from the owner and from the locations the lookup returns.

**Rejected**
A caching relay was considered and rejected because it needs a freshness rule for the cache and a way to stop an old snapshot from circulating.

#### Profiles are refreshed by pull

**Decision**
A followed Profile is fetched on demand and again after the refresh period; there is no push.

**Rationale**
Pull keeps the order of updates at the receiver, and it needs no announcement channel or spam control.
The cost is that an update waits for the next refresh or a manual fetch.

### 7.2 Deferred

#### Serving cached Profiles

**Current state**
Only the owner serves a Profile.

**Why not decided yet**
A cache needs a freshness rule, a way to stop an old snapshot from spreading, and a storage bound, and the owner's availability is enough for the first version.

**Decision condition**
Decide when owner availability is the reason a followed Profile cannot be fetched.

#### Push of Profile updates

**Current state**
A node learns of an update only by fetching again.

**Why not decided yet**
A push needs a subscription, a rate limit, and a rule for an announcement that arrives before the body.

**Decision condition**
Decide when the refresh period is too slow for the trust evaluation to react to an update.

#### Transferring a Profile larger than 1 MiB

**Current state**
A Profile over 1 MiB is rejected.

**Why not decided yet**
The list bounds keep a valid Profile below the limit, and a larger body needs a chunk protocol and a receiver-side bound.

**Decision condition**
Decide when a real Profile cannot fit the limits of [profile.md](./profile.md#7-limits).

## 8. Current status and remaining work

The design is not implemented.
The engine has an empty module placeholder and no protocol, and the daemon does not advertise or dial a Profile.

The implementation order is in [design.md §6.2](../design.md#62-roadmap).

For confirmed bugs, see [issues.md](../issues.md).
