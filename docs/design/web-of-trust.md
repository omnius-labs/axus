# Design of the Web of Trust

## 1. About this document

This document covers how a node decides whether to trust the author of a Profile, the FileRefs it lists, and the messages it posts.
The Profile body and its lists belong to [profile.md](./profile.md#1-about-this-document), and the distribution of Profiles to [profile-exchanger.md](./profile-exchanger.md#1-about-this-document).
This document assumes the vocabulary of [terms.md](../terms.md). Term definitions are not given here.

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | The evaluation of the trust and block lists and the states derived from them |
| [profile.md](./profile.md#1-about-this-document) | Profile identity, the signed body, and the lists themselves |
| [profile-exchanger.md](./profile-exchanger.md#1-about-this-document) | Lookup, exchange, and refresh of Profiles |
| [trust-security.md](./trust-security.md#2-responsibilities-and-boundaries) | What the Session authenticates and what it does not |
| [file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries) | Publishing and subscribing to the files a FileRef names |
| [terms.md](../terms.md) | Term definitions |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §8.
To find out what runs now, read §8 first.

## 2. Responsibilities and boundaries

The evaluation turns the trust and block lists of fetched Profiles into a state for each Profile and each piece of content.
Every node computes its own result, and no node can decide another node's result.

This document does not cover transport authentication.
The Session proves that the peer holds the node key ([trust-security.md](./trust-security.md#3-division-of-guarantees-and-threats)), and that proof says nothing about whether the content the peer serves is trustworthy.
This document also does not adopt friend-to-friend connection gating: the daemon accepts a Session from any peer, and the trust result is applied to content, not to admission ([terms.md](../terms.md)).

The input of the evaluation is the Profile body, and the output is a state per Profile and per piece of content.
The evaluation does no network I/O; the Profiles are fetched by [profile-exchanger.md](./profile-exchanger.md#1-about-this-document).

## 3. Trust data

### 3.1 The lists

The trust list and the block list are the only inputs of the evaluation ([profile.md](./profile.md#43-trust-list-and-block-list)).
A trust entry is a positive statement about another Profile ID, and a block entry is a negative one.
The entries carry no weight, no expiry, and no note; the lists are sets.

A Profile's own lists are statements by its author, and the author's signature is the only reason to read them.
A Profile with an invalid signature is discarded at decode ([profile.md](./profile.md#41-fields)) and contributes no edge.

### 3.2 Building the graph

The evaluation starts from the active Profile ([profile.md](./profile.md#33-several-profiles-on-one-node)) and expands its trust list.
The Profiles named by the trust list are fetched through the ProfileExchanger, and their lists are read in turn.
A Profile that cannot be fetched stays unknown; its edges are not guessed.

The graph is rebuilt from the stored snapshots whenever a Profile is updated.
The result is a function of the stored snapshots and the active Profile, so two evaluations with the same inputs produce the same states.

## 4. Evaluation

### 4.1 The root

The active Profile is the root and distance 0.
Its trust and block lists produce the distance-1 candidates, and the trust and block lists of the accepted distance-1 Profiles produce the distance-2 candidates.
No other Profile contributes, even when it is stored locally.

The root is a local choice.
Two nodes with different active Profiles can reach different results for the same Profile, and that difference is the point of a local evaluation.

### 4.2 Distance and the expansion

Profiles are expanded in distance order.
The candidates at distance d+1 are the union of the trust entries and the block entries of the accepted Profiles at distance d, and the root's two lists produce the distance-1 candidates.
A candidate named by a block entry is `distrusted` and is not expanded; a candidate named by a trust entry is `trusted` or `trusted via` unless a block entry also names it, in which case the block wins.
The block lists of distance-2 Profiles are not applied, because their targets would be at distance 3, outside the horizon.
A Profile reached by two paths keeps the shorter distance.

The expansion stops at distance 2, at 256 fetched Profiles, and at the evaluation deadline.
Whatever was not expanded by then stays `unknown`; the truncation never turns an unexpanded Profile into a trusted one.

### 4.3 States

| State | Meaning |
| --- | --- |
| `trusted` | Named by the active Profile's trust list and not blocked |
| `trusted via` | Reached at distance 2 through a trusted Profile and not blocked |
| `unknown` | Not reached within the limits |
| `distrusted` | Removed by an applied block list (§4.2) |

A block from the root applies to the distance-1 candidates, and a block from a distance-1 Profile applies to the distance-2 candidates only; a distance-1 Profile is removed only by the root's block list.
When an ID is named by an applied block list and by a trust list, the block wins.

A file or a message whose claiming Profile is none of the four is `unattributed`: no fetched Profile lists it, so there is no author to judge.
An unattributed file is not the same as a trusted one, and the client must not imply authorship.

### 4.4 Block precedence

A block applies when its author is the root or a Profile at distance 1 that was accepted.
A block from a distance-2 Profile is not applied, because its target would be at distance 3; a block from an `unknown` or `distrusted` Profile is ignored.

The rule is fail-closed: a block from an applied Profile removes the target from the next layer, and no trust path from another applied Profile restores it.
The depth limit bounds the reach of a compromised Profile; a key one hop away can still block, and that cost is accepted to keep one rule for trust and block.

### 4.5 Freshness and limits

Every fetched Profile carries its fetch time, and a snapshot that a newer one replaces is dropped ([profile.md](./profile.md#52-replacement-and-rollback)).
The evaluation uses the stored snapshots as they are; a Profile that has not been refreshed for longer than the refresh period is still used, and the client shows its age.

The limits of §4.2 are the only bounds on the work.
They keep a single client from fetching the whole reachable graph, and they make the result of a truncated evaluation explicit as `unknown`.

## 5. Application

### 5.1 Files

A file is attributed to every Profile whose FileRef list names its root hash and name.
When one Profile claims it, the file carries that Profile's state.
When several claim it, the client shows every claimant with its own state, and the file takes the best state among them; a `distrusted` claim never raises the file's state.
When every claimant is `distrusted`, the file is `distrusted`.

A file whose root hash and name are listed by no fetched Profile is `unattributed`.
The file protocol still verifies every block against the root hash ([file-transfer.md](./file-transfer.md#57-verification-and-violations)), so an unattributed file is not corrupt; it simply has no author to judge.

### 5.2 Messages

A message is attributed to the Profile that lists it, and the Profile's state applies.
A message is not evaluated on its own, because the Profile's signature covers the list and the body together.

### 5.3 Subscription and search

A subscription is always allowed.
The client shows the author and the state before and during the fetch, and a `distrusted` author is shown as a warning, not as a blocked operation.
This keeps a share from one node usable by a node that has not judged the author yet, and it leaves the decision with the operator.

A future search may filter by state, such as showing only `trusted` and `trusted via` results.
The rule is defined here, but the search itself is not designed.

## 6. Threats and limits

**Sybil.** Keys are cheap, so an attacker can create any number of Profiles.
A newcomer has no path from the root and stays `unknown`; becoming `trusted` requires an accepted Profile to trust it, which is the cost the design relies on.
The depth limit keeps a distant cluster from spending the fetch budget.

**Impersonation.** A Profile ID carries a key-derived suffix ([profile.md](./profile.md#31-profile-id)), so a same-name Profile with another key is visibly a different ID.
The signature check rejects a body whose key does not produce the ID.

**Key loss.** A lost profile key cannot be replaced without a new identity ([profile.md](./profile.md#82-deferred)).
The old ID remains trusted but stops updating, and the new one is `unknown` until the owner asks for trust.

**No global score.** The result is a state computed by each node, not a number that all nodes agree on.
A client cannot show "the network trusts this author", only "this node trusts this author".

**Block abuse.** The root or a Profile at distance 1 can block widely.
The design accepts this because the operator chose to trust it, and distance 2 bounds how many keys can apply a block.

**Stale Profiles.** A Profile that is not refreshed keeps its state.
The client shows the fetch time, and a later refresh can change the result.

## 7. Design decisions

### 7.1 Decided

#### Trust is evaluated locally from signed Profiles

**Decision**
Each node computes the states from the Profiles it has fetched; there is no shared score and no vote.

**Rationale**
A shared score would need a global view and would be easy to Sybil, while a local evaluation gives every operator a result they can explain from their own trust list.

#### The evaluation expands to distance 2

**Decision**
The root, its trusted Profiles, and their trusted Profiles contribute; nothing further does.

**Rationale**
One hop only repeats a list the operator already has, and three hops crosses from "someone I trust trusts this" into a crowd the operator cannot reason about.
Two hops covers the common introduction case and bounds the fetch work.

**Rejected**
An unbounded expansion was considered and rejected because it would let a distant cluster consume the fetch budget.
A weighted score with decay was considered and rejected because the number would be hard to justify and easy to game.

#### The result is a state, not a score

**Decision**
A Profile is `trusted`, `trusted via`, `unknown`, or `distrusted`, and content is additionally `unattributed` when no Profile claims it.

**Rationale**
The operator can act on a state without knowing a formula, and the client can explain why a Profile got it.
A score would invite thresholds that no one can defend.

#### A block wins over trust

**Decision**
A block entry from the root or a Profile at distance 1 removes the target from the next layer, and no trust path restores it.

**Rationale**
A negative statement is the operator's protection against a bad introduction, and letting a second path cancel it would make the result depend on the order of arrival.
Fail-closed keeps the rule independent of the traversal order, and applying a block only to the next layer keeps the evaluation deterministic.

#### Content is judged through its claiming Profile

**Decision**
A FileRef and a message inherit the state of the Profile that lists them.

**Rationale**
The Profile's signature covers the list and the content reference together, so the author of the claim is the unit of trust.

#### A subscription is warned, not blocked

**Decision**
An `unknown`, `distrusted`, or `unattributed` author produces a warning and never stops the subscription.

**Rationale**
Search does not exist yet, and a share from a new author must be usable before any trust path exists.
A strict mode that refuses untrusted authors is deferred until there is a search that can supply trusted candidates.

### 7.2 Deferred

#### A strict mode that refuses untrusted authors

**Current state**
Every subscription is allowed and the author's state is shown.

**Why not decided yet**
Refusing by default would make a first share from a new author unusable, and no search exists to offer a trusted alternative.

**Decision condition**
Decide when search can rank or filter by state and the operator has a way to find a trusted copy.

#### Search filtering by state

**Current state**
The rule for filtering is defined, and the search itself is not designed.

**Why not decided yet**
The search needs its own index, ranking, and result model, which is a separate concern.

**Decision condition**
Decide before the first search API that returns files or Profiles.

#### Expansion beyond distance 2

**Current state**
The expansion stops at distance 2.

**Why not decided yet**
A larger radius needs a decay or a score to stay meaningful, and the current states have no way to express it.

**Decision condition**
Decide when real introductions at distance 2 do not cover the common case.

## 8. Current status and remaining work

The design is not implemented.
The daemon has no Profile, no ProfileExchanger, and no trust evaluation; the client has no trust display.

The implementation order is in [design.md §6.2](../design.md#62-roadmap).

For confirmed bugs, see [issues.md](../issues.md).
