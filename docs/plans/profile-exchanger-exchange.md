# Plan: ProfileExchanger lookup, exchange, and serving

## 1. About This Plan

This plan implements the lookup, exchange, and serving parts of [profile-exchanger.md](../design/profile-exchanger.md#1-about-this-document).
A node advertises the Profiles it owns, finds another node by a Profile ID, fetches the signed body over a dedicated Session use, and stores it apart from its own Profiles.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| The protocol, limits, serving rules, and the reasons for the decisions | [profile-exchanger.md](../design/profile-exchanger.md#1-about-this-document) |
| The signed body and its update rules | [profile.md](../design/profile.md#1-about-this-document) |
| The storage layout of owned and fetched Profiles | [storage.md](../design/storage.md#48-profile-storage) |
| Lookup and active lookup | [node-finder.md](../design/node-finder.md#1-about-this-document) |
| Session establishment and the use selection | [session.md](../design/session.md#1-about-this-document) |
| Vocabulary | [terms.md](../terms.md) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-11-15 against commit `0edca6e`.

## 2. Purpose and Completion Criteria

A node can turn a Profile ID into a signed Profile body without trusting the serving node's word, and the Profiles it owns can be found and fetched by other nodes.

### 2.1 Completion Criteria

1. A node advertises the AssetKeys of the Profiles it owns, and another node's lookup returns the owner's NodeProfile.
2. A node fetches a Profile by ID over a `ProfileExchanger` Session, verifies the body, and stores it in the fetched table.
3. A serving node answers with a Profile only when it owns that Profile; a fetched or unknown ID is answered as not-found.
4. A body that fails signature, ID, or update-order verification is not stored, the dialer tries the next location, and the location is not excluded.
5. A Profile fetched before a restart is still readable after the restart, and does not appear among the owned Profiles.
6. The `ProfileExchanger` read path uses the dedicated Session use and the 1 MiB plus 256 byte frame limit.
7. The accept and fetch paths share one limit of 8 concurrent Sessions.
8. Regenerating the wire types produces the committed generated artifacts without a diff.
9. `AxusService` starts and stops `ProfileExchanger`; a failed startup returns an error without a running worker, shutdown joins the workers, and the public methods fetch a Profile by ID and read a stored fetched Profile.
10. `cargo make lint` and `cargo make test` pass.

### 2.2 Non-Goals

| Item | Where it goes |
| --- | --- |
| Periodic refresh and the followed set | [profile-exchanger.md §5](../design/profile-exchanger.md#5-refresh), to be planned with the Web of Trust |
| Serving a fetched Profile | [profile-exchanger.md §7.1](../design/profile-exchanger.md#only-the-owner-serves-a-profile) |
| REST API and UI for Profiles | [daemon-api.md](../design/daemon-api.md#5-rest-resources), [web-client.md](../design/web-client.md#3-screens-and-routes) |
| Appending FileRefs at a publish commit | [profile.md §6](../design/profile.md#6-publishing-through-a-profile) |
| Chunked Profiles over 1 MiB | [profile-exchanger.md §7.2](../design/profile-exchanger.md#transferring-a-profile-larger-than-1-mib) |
| File search | [web-of-trust.md §5.3](../design/web-of-trust.md#53-subscription-and-search) |

## 3. Current State

`core/negotiator/profile_exchanger.rs` is a one-byte placeholder, and `core/negotiator.rs` only declares and re-exports it ([negotiator.rs](../../daemon/modules/engine/src/core/negotiator.rs#L3)).
There is no wire type, no Session use, no lookup, no serving, and no storage for a fetched body.

`LocalProfiles` owns the Profiles with keys under `state_dir/profile` ([local_profiles.rs](../../daemon/modules/engine/src/core/profile/local_profiles.rs#L16)).
Its repository has one `profiles` table for owned Profiles and rejects an owned body without a key at startup ([repo.rs](../../daemon/modules/engine/src/core/profile/repo.rs#L55)).
Storing a fetched body in that table would make it indistinguishable from an owned Profile, and startup would reject it.

`AssetKey` has only the `file` type ([asset_key.rs](../../daemon/modules/engine/src/model/asset_key.rs#L19)), and `SessionType` has only `NodeFinder` and `FileExchanger` ([model.rs](../../daemon/modules/engine/src/core/session/model.rs#L16); [message.rs](../../daemon/modules/engine/src/core/session/message.rs#L15)).
`NodeFinder` exposes the location cache through `find_node_profile` ([node_finder.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L244)), but does not expose the active lookup of `TaskLookup` ([task_lookup.rs](../../daemon/modules/engine/src/core/negotiator/node/task_lookup.rs#L141)).
`AxusService` creates `LocalProfiles` and passes only `NodeFinder` and `FileExchanger` to the accepter ([service.rs](../../daemon/modules/engine/src/service.rs#L46); [service.rs](../../daemon/modules/engine/src/service.rs#L116)).

The missing pieces are a Profile-specific lookup, a dedicated Session use, fetched-body storage, and the exchanger that composes them.
Without them a Profile can be signed and stored locally, but no other node can find or fetch it.

## 4. Change Approach

The wire and the Session use come first, then the fetched table, then the exchanger, and finally the service wiring.
The lookup is made explicit on `NodeFinder` so that the exchanger does not have to wait for a periodic want cycle.
The fetched table is separate from the owned table, so serving a fetched body is impossible without an explicit new decision.

### 4.1 Assumed Decisions

| Decision | Source |
| --- | --- |
| A Profile is looked up under an AssetKey of type `profile` whose hash is SHA3-256 of the ID string | [profile-exchanger.md §3.1](../design/profile-exchanger.md#31-the-assetkey) |
| ProfileExchanger has its own Session use | [profile-exchanger.md §4.1](../design/profile-exchanger.md#41-the-exchange-use) |
| A frame carries one Profile and is limited to 1 MiB plus 256 bytes | [profile-exchanger.md §7.1](../design/profile-exchanger.md#one-profile-per-frame) |
| Only the owner advertises and serves a Profile | [profile-exchanger.md §7.1](../design/profile-exchanger.md#only-the-owner-serves-a-profile) |
| A fetched Profile is stored apart from the owned Profiles and is never served | [profile-exchanger.md §7.1](../design/profile-exchanger.md#fetched-profiles-are-stored-apart-from-owned-profiles) |
| The Profile update rules decide which snapshot is current | [profile.md §5](../design/profile.md#5-update-rules) |
| NodeFinder answers a lookup with the nodes closest to the AssetKey | [node-finder.md §6.1](../design/node-finder.md#61-decided) |
| The Session use selection and frame limit switch are owned by the Session layer | [session.md §5.1](../design/session.md#51-decided) |

### 4.2 Scope of Impact

Touched: `daemon/rpfs/profile.rpf` and the generated code, `modules/engine/src/protocol/profile.rs`, `modules/engine/src/model/asset_key.rs`, `modules/engine/src/model/profile.rs`, `modules/engine/src/core/profile/repo.rs`, `modules/engine/src/core/profile/local_profiles.rs`, `modules/engine/src/core/negotiator/profile_exchanger.rs`, `modules/engine/src/core/negotiator/node/node_finder.rs`, `modules/engine/src/core/session/model.rs`, `message.rs`, `accepter.rs`, `connector.rs`, `modules/engine/src/service.rs`, and the design documents.

The wire change adds a new use and new message type; it does not change an existing message.
An old daemon rejects the new use during the use selection, so the two versions can still connect for NodeFinder and FileExchanger.
The storage change adds a `fetched_profiles` table; it does not rewrite the `profiles` table.
No REST API, UI, publication, or trust-evaluation behavior changes.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [1](#s-1) | Wire types and generated code for the Profile exchange | | done |
| [2](#s-2) | Profile AssetKey and the active NodeFinder lookup | | done |
| [3](#s-3) | Protocol codec and the ProfileExchanger Session use | 1 | done |
| [4](#s-4) | Fetched Profile storage | | done |
| [5](#s-5) | Serving and the location finder | 2, 3, 4 | done |
| [6](#s-6) | Fetch, concurrency, and shutdown | 5 | done |
| [7](#s-7) | AxusService wiring and advertisement | 6 | done |
| [8](#s-8) | End-to-end tests and status close-out | 7 | in progress |

<a id="s-1"></a>
### Step 1. Wire types and generated code for the Profile exchange

**Locations to change**
New `daemon/rpfs/profile.rpf`; regenerate [generated.rs](../../daemon/modules/engine/src/generated.rs#L1) and the generated modules with [gen-rocketpack.sh](../../daemon/gen-rocketpack.sh#L1).

**What to do**
Define package `axus::profile` with an `ExchangeMessage` enum carrying `Request { profile_id }`, `Response { profile }`, and `NotFound { profile_id }`.
The Profile ID is a string bounded by 80 bytes, and `Profile` comes from `axus::model`.
The frame limit is defined in the codec as `MAX_ENCODED_PROFILE_LENGTH + 256`, not in the rpf.

**Completion condition**
The generated module contains `generated::axus::profile::ExchangeMessage`, regeneration produces no diff, and the new types compile.

<a id="s-2"></a>
### Step 2. Profile AssetKey and the active NodeFinder lookup

**Locations to change**
[asset_key.rs](../../daemon/modules/engine/src/model/asset_key.rs#L19); [node_finder.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L244).

**What to do**
Add the `profile` AssetKey type and `AssetKey::profile(&ProfileId)` that hashes the ID's textual form with SHA3-256.
Add `NodeFinder::lookup(&AssetKey)` that clones the `TaskLookup` and calls its active `lookup`, returning an error when the lookup task is not running.

**Completion condition**
A unit test fixes the `profile` type, the SHA3-256 hash of a known ID, and the difference from the `file` type.
A test or the existing lookup tests show that `NodeFinder::lookup` reaches `TaskLookup::lookup`.

<a id="s-3"></a>
### Step 3. Protocol codec and the ProfileExchanger Session use

**Locations to change**
New `modules/engine/src/protocol/profile.rs`; [protocol.rs](../../daemon/modules/engine/src/protocol.rs#L1); [session/model.rs](../../daemon/modules/engine/src/core/session/model.rs#L16); [session/message.rs](../../daemon/modules/engine/src/core/session/message.rs#L15); [session/accepter.rs](../../daemon/modules/engine/src/core/session/accepter.rs#L266); [session/connector.rs](../../daemon/modules/engine/src/core/session/connector.rs#L85).

**What to do**
Add `ProfileExchangeMessageCodec` for the generated enum and a `MAX_FRAME_LENGTH` of `Profile::MAX_ENCODED_LENGTH + 256` ([profile.rs](../../daemon/modules/engine/src/model/profile.rs#L47)).
Add `SessionType::ProfileExchanger` and `V1RequestType::ProfileExchanger = 3`, map both handshake directions, and switch the frame limit after use selection.

**Completion condition**
The codec round-trips Request, Response, and NotFound, and rejects a request ID over 80 bytes.
The Session tests select `ProfileExchanger`, apply its frame limit, and reject an unsupported use result.

<a id="s-4"></a>
### Step 4. Fetched Profile storage

**Locations to change**
[profile/repo.rs](../../daemon/modules/engine/src/core/profile/repo.rs#L55); [local_profiles.rs](../../daemon/modules/engine/src/core/profile/local_profiles.rs#L16).

**What to do**
Add a `fetched_profiles` migration with `id`, `value`, `version`, `updated_time`, and `fetched_time`.
Add `replace_fetched_if_newer` and `get_fetched` to the repository.
A newer body replaces the row and updates the fetch time; the same body updates the fetch time; an older body and an equivocation keep both the body and the fetch time.
Expose the operations through `LocalProfiles` without making the fetched rows visible to `get`, `list`, or `active`.

**Completion condition**
Tests show replacement, same-body fetch-time reset, older-body retention, equivocation retention, restart survival, and that a fetched row is absent from the owned list and from activation.

<a id="s-5"></a>
### Step 5. Serving and the location finder

**Locations to change**
[profile_exchanger.rs](../../daemon/modules/engine/src/core/negotiator/profile_exchanger.rs#L1); new `modules/engine/src/core/negotiator/profile_exchanger/` files for the location finder, the serve worker, the accepter, and the option/intervals.

**What to do**
Add `ProfileLocationFinder` with `my_node_id`, `set_provided`, and `find`.
The NodeFinder implementation registers the provided AssetKeys with the push listener and calls `NodeFinder::lookup` for `find`.
The serve worker accepts Requests until the peer closes, answers only owned Profiles, and answers NotFound for every other ID.
The accepter takes a permit from the shared Session semaphore before accepting and joins its workers on shutdown.

**Completion condition**
A loopback test fetches an owned Profile from a serving node, receives NotFound for a fetched or unknown ID, and observes the Session close after a malformed request ID.
A test shows the push listener sees the owned Profile AssetKeys and the fetch path calls lookup.

<a id="s-6"></a>
### Step 6. Fetch, concurrency, and shutdown

**Locations to change**
The same new `profile_exchanger` files.

**What to do**
Implement `ProfileExchanger::fetch(&ProfileId)`: return an owned Profile directly, otherwise look up locations, dial each location with the dedicated use, verify the peer key, send Request, verify the Response body, its ID, and its update order, store it, and return it.
A NotFound answer moves to the next location; a bad body or unexpected message is discarded and also moves to the next location without excluding the node.
Each fetch Session holds a permit from the same shared semaphore as the accepter, and shutdown stops the accepter and workers.
Read a stored fetched Profile without network access through `ProfileExchanger::stored`.

**Completion condition**
Loopback tests cover a successful fetch, a fallback from NotFound to the next location, a bad body or stale snapshot that is not stored and does not exclude the location, and the shared Session limit.
A restart test shows that `stored` returns the fetched Profile without a peer.

<a id="s-7"></a>
### Step 7. AxusService wiring and advertisement

**Locations to change**
[service.rs](../../daemon/modules/engine/src/service.rs#L46); [service.rs](../../daemon/modules/engine/src/service.rs#L116); [service.rs](../../daemon/modules/engine/src/service.rs#L262).

**What to do**
Wrap `LocalProfiles` in `Arc`, create `ProfileExchanger` after `FileExchanger`, add `ProfileExchanger` to the accepter's supported uses, and refresh the provided Profile AssetKeys at startup and after `create_profile`.
Add `fetch_profile` and `stored_profile` delegates and shut the exchanger down before the NodeFinder.

**Completion condition**
Two `AxusService` nodes over loopback fetch a Profile created on one by ID, and a fresh service reads it from the fetched table after restart.
Startup fails cleanly when the exchanger cannot start, and shutdown leaves no running worker.

<a id="s-8"></a>
### Step 8. End-to-end tests and status close-out

**Locations to change**
[profile-exchanger.md](../design/profile-exchanger.md#8-current-status-and-remaining-work); [design.md](../design.md#6-current-status-and-remaining-work); [plans.md](../plans.md#2-list).

**What to do**
Record the implemented scope and the deferred refresh in the design status, remove the plan row, and fold this plan after the acceptance verification.
Keep the plan's non-goals in the design documents or the issue list as required by the writing norm.

**Completion condition**
The design status names lookup, exchange, serving, fetched storage, and the deferred refresh, the plan is removed, and no plan reference remains outside `docs/plans/`.

## 6. Acceptance Verification

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_asset_key|profile_exchanger.*location/)'` | The owned AssetKeys reach the push listener and the lookup returns the owner |
| 2 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_exchanger.*fetch/)'` | The fetched Profile is verified and stored |
| 3 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_exchanger.*serve|profile_exchanger.*not_found/)'` | Owned IDs are served and fetched or unknown IDs get NotFound |
| 4 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_exchanger.*bad_body|profile_exchanger.*bad_id|profile_exchanger.*stale|profile_exchanger.*equivocation/)'` | The bad body, mismatched ID, stale snapshot, and equivocation are not stored, the next location is tried, and the location is not excluded |
| 5 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/fetched_profile|profile_exchanger.*restart/)'` | Fetched rows survive a restart and stay out of the owned list |
| 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/session::tests|protocol::profile/)'` | The use selection, frame limit, and codec tests pass |
| 7 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_exchanger.*session_limit/)'` | The accept and fetch paths never hold more than 8 Sessions together |
| 8 | `cd daemon && snapshot="$(mktemp -d "${AGENT_TEMP_DIR:-/tmp}/axus-generated.XXXXXX")" && cp -R modules/engine/src/generated "$snapshot/generated" && cp modules/engine/src/generated.rs "$snapshot/generated.rs" && ./gen-rocketpack.sh && diff -ru "$snapshot/generated" modules/engine/src/generated && diff -u "$snapshot/generated.rs" modules/engine/src/generated.rs; status=$?; rm -rf "$snapshot"; exit $status` | Exit code 0 with no diff |
| 9 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/service::tests.*profile_exchanger/)'` | Two services fetch a Profile by ID and the restart read succeeds; a failed startup leaves no running worker; shutdown joins the workers |
| 10 | `cd daemon && RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo make test` | Exit code 0, all tests pass |

## 7. Risks and Rollback

The storage change adds a table and does not rewrite existing rows.
Reverting the code leaves the table unused, and no stored Profile is lost.
The wire change adds a use and a message type without changing an existing message; an old daemon rejects the new use and continues to serve NodeFinder and FileExchanger.
Reverting the code removes the use and the message type, and the deployment returns to the previous wire without a migration.
