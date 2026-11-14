# Plan: Profile foundation

## 1. About This Plan

This plan implements the Profile identity, signed body, key, storage, update rules, and startup activation described in [profile.md](../design/profile.md#1-about-this-document).
It does not implement the ProfileExchanger, the Web of Trust, the REST API, the browser client, or publication integration.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| Profile identity, the signed body, the lists it holds, and the update rules | [profile.md](../design/profile.md#1-about-this-document) |
| Profile key, signed body, and active-flag storage | [storage.md](../design/storage.md#48-profile-storage) |
| Vocabulary | [terms.md](../terms.md#2-terminology) |
| Confirmed bugs | [issues.md](../issues.md) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-11-13 against commit `ce1b186`.

## 2. Purpose and Completion Criteria

A Profile can be created, signed, verified, stored, loaded, updated, and selected as active, and a fresh daemon starts with an active `Anonymous` Profile.
The foundation gives the later ProfileExchanger and Web of Trust a signed body to exchange and evaluate without exposing Profiles over the REST API or the network.

### 2.1 Completion Criteria

1. A fresh state directory has an active `Anonymous` Profile with a key at `state_dir/profile/keys/<Profile ID>/signer`, a signed body in `state_dir/profile/sqlite.db`, and owner-only key permissions on Unix.
2. The `ProfileId` is recomputed from the public key in the signed body, and a body whose ID does not match its key, whose name is empty or invalid, or whose name or ID exceeds its limit is rejected.
3. `ProfileService` creates, lists, gets, activates, and updates Profiles; the active Profile survives a restart; an update increments the version, sets the update time to now, signs the new body, and is serialized against another update; an update whose current time is earlier than the stored update time is rejected.
4. A newer snapshot replaces an older one, an older snapshot is ignored, an equal update time uses the higher version, an update time more than 5 minutes ahead is rejected, and an equal time and version with a different body keeps the first and logs the second.
5. A Profile cannot trust or block itself, the same Profile ID cannot appear in both lists, and the trust, block, FileRef, message, inline-body, and encoded-Profile limits are enforced.
6. At startup, existing Profiles are not duplicated; when no Profile is active, the oldest by creation time and then by Profile ID is activated; each key without a body is recovered as an empty Profile; and a body without a key fails startup.
7. `AxusService` starts and stops with the Profile service and exposes it through an accessor; no REST, UI, publication, exchange, or trust code changes.
8. `cargo make lint` and `cargo make test` pass; the status sections of [profile.md](../design/profile.md#9-current-status-and-remaining-work) and [design.md](../design.md#61-current-status) state that the Profile foundation is implemented and the ProfileExchanger, Web of Trust, publication integration, and API/UI remain; and [storage.md §6](../design/storage.md#6-current-status-and-remaining-work) lists the Profile repository and key store.
9. The implementation branch passes the Ubuntu, macOS, and Windows CI workflow before merge.

### 2.2 Non-goals

| Item | Where it goes |
| --- | --- |
| REST/OpenAPI resources and the browser Profile screen | Not part of this work; see [daemon-api.md §10](../design/daemon-api.md#10-current-status-and-remaining-work) and [web-client.md §7](../design/web-client.md#7-current-status-and-remaining-work) |
| Appending and removing FileRefs at a publish commit | [profile.md §9](../design/profile.md#9-current-status-and-remaining-work); later plan |
| ProfileExchanger lookup, exchange, and refresh | [design.md §6.2](../design.md#62-roadmap) item 2 |
| Web of Trust evaluation and trust display | [design.md §6.2](../design.md#62-roadmap) item 3 |
| File search | [web-of-trust.md §5.3](../design/web-of-trust.md#53-subscription-and-search) |
| Profile key rotation, renaming, and metadata beyond the lists | [profile.md §8.2](../design/profile.md#82-deferred) |
| Deleting a local Profile | [profile.md §8.2](../design/profile.md#82-deferred) |

## 3. Current State

`FileRef` is a generated wire type re-exported by [file_ref.rs](../../daemon/modules/engine/src/model/file_ref.rs#L1), and [node_profile.rs](../../daemon/modules/engine/src/model/node_profile.rs#L38) shows the existing signed-model pattern: a fixed signing prefix, a RocketPack signing payload, an `OmniSigner` signature, and a verification step that reconstructs the cert.

[identity.rs](../../daemon/modules/engine/src/core/identity.rs#L14) stores the node key at `state_dir/identity/signer`, writes it through a temporary file, and sets `0600` on Unix ([identity.rs](../../daemon/modules/engine/src/core/identity.rs#L40)).
[service.rs](../../daemon/modules/engine/src/service.rs#L88) loads that identity and creates NodeFinder and FileExchanger; it creates no Profile.

[model.rpf](../../daemon/rpfs/model.rpf#L13) defines FileRef and NodeProfile but no Profile.
[profile_exchanger.rs](../../daemon/modules/engine/src/core/negotiator/profile_exchanger.rs#L1) is an empty placeholder.
[storage.md §3.1](../design/storage.md#31-structure-of-state_dir) describes the state directory, and [§4.8](../design/storage.md#48-profile-storage) defines the Profile layout, but no code implements it.

Without these, a Profile cannot be created, signed, stored, or updated, and the exchange and trust layers have nothing to build on.

## 4. Change Approach

The wire types and limits come first, then the Profile ID and signed model, then the key store and SQLite repository, and finally the service and daemon wiring.
The signed body is the exported RocketPack struct stored as a blob; the key stays in a file outside SQLite.
`ProfileService::open` performs the startup creation and recovery, so every daemon and engine test exercises the same path.

### 4.1 Assumed Decisions

The core-rs `OmniSigner` and `OmniCert` name rule is an external prerequisite.
It enforces 0–32 bytes of `[A-Za-z0-9._-]`, allows the empty name for the NodeProfile certificate, and changes both `OmniSigner.name` and `OmniCert.name` to `string[..=32]`.
The Axus Profile ID layer separately requires a non-empty 1–32 byte display part.
Step 1 bumps the submodule after that core-rs PR merges.

| Decision | Source |
| --- | --- |
| Profile ID format and name rule | [terms.md](../terms.md#2-terminology), [profile.md §3.1](../design/profile.md#31-profile-id) |
| Signed body fields, messages, and limits | [profile.md §4](../design/profile.md#4-the-signed-body), [§7](../design/profile.md#7-limits) |
| Update rules, including local updates | [profile.md §5](../design/profile.md#5-update-rules) |
| Startup creation and active Profile | [profile.md §6](../design/profile.md#6-publishing-through-a-profile) |
| Key, body, and active-flag storage and recovery | [storage.md §4.8](../design/storage.md#48-profile-storage) |
| SQLite durability conventions | [storage.md §4.2](../design/storage.md#42-durability-guarantees-and-boundaries) |

### 4.2 Scope of Impact

Touched: `daemon/rpfs/model.rpf` and its generated code, `daemon/modules/engine/src/model/` (new `profile.rs` and `profile_id.rs`), `daemon/modules/engine/src/core/profile/` (new module), `daemon/modules/engine/src/service.rs`, `docs/design/profile.md`, `docs/design.md`, `docs/plans.md`, and this plan.
The implementation branch is `feat/profile-foundation`.

Affected without edits: a fresh `AxusService` creates `state_dir/profile` and an `Anonymous` Profile, so engine tests and daemon startup see the new state. Existing state directories without `profile/` are unchanged; the next start creates the Profile. No REST, UI, publication, exchange, or trust code changes.

External: the core-rs submodule bump narrows `OmniSigner.name` and `OmniCert.name` from 0–256 to 0–32 bytes; the empty name is allowed. The daemon is pre-release and no migration is provided. The new Profile wire types are not used by an existing message, so no existing Axus wire message changes.

New engine names: `ProfileId`, `ProfileService`, and `ProfileDraft`. The terms `Profile ID` and `active Profile` are recorded in [terms.md](../terms.md#2-terminology). No REST names are added.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [1](#s-1) | Bump the core-rs submodule | | not started |
| [2](#s-2) | Profile wire types and limits | 1 | not started |
| [3](#s-3) | Profile ID type and validation | 2 | not started |
| [4](#s-4) | Signed Profile model | 1, 2, 3 | not started |
| [5](#s-5) | Profile key store | 3, 4 | not started |
| [6](#s-6) | Profile SQLite repository | 4 | not started |
| [7](#s-7) | Profile service and startup behavior | 5, 6 | not started |
| [8](#s-8) | AxusService wiring and status update | 7 | not started |

<a id="s-1"></a>
### Step 1. Bump the core-rs submodule

**Locations to change**
`daemon/refs/core-rs` (submodule pointer), and `daemon/Cargo.lock` only if the path dependency changes it.

**What to do**
After the core-rs `OmniSigner` name-rule PR merges, point the submodule at that commit and update the lockfile if required.
Do not change the core-rs source from this repository.

**Completion criterion**
`git submodule status daemon/refs/core-rs` shows the merged core-rs commit, the core-rs name tests pass there, and `RUSTC_WRAPPER= cargo make lint` passes in `daemon`.

<a id="s-2"></a>
### Step 2. Profile wire types and limits

**Locations to change**
[model.rpf](../../daemon/rpfs/model.rpf#L1), the generated [model.rs](../../daemon/modules/engine/src/generated/axus/model/model.rs#L1), and [gen-rocketpack.sh](../../daemon/gen-rocketpack.sh#L1).

**What to do**
Add a `Profile` wire struct with the ID, public key, version, update time, trust list, block list, FileRefs, messages, and signature.
Add a `ProfileSigningPayload` with the same fields except the signature, a `ProfileMessage` with a posted time and a body, and a `ProfileMessageBody` enum with inline bytes and a FileRef variant.
Add the limits from profile.md §7 and regenerate with `./gen-rocketpack.sh` from `daemon/`.

**Completion criterion**
The generated types round-trip, the wire validation rejects an ID over 80 bytes, too many list entries, an inline body over 64 KiB, and too many messages, and no existing wire message changes.

<a id="s-3"></a>
### Step 3. Profile ID type and validation

**Locations to change**
New `daemon/modules/engine/src/model/profile_id.rs` and the module export in [model.rs](../../daemon/modules/engine/src/model.rs#L1).

**What to do**
Implement `ProfileId` as a parsed value with display, parsing, comparison, and hashing.
Validate the name with the core-rs rule, then reject an empty display part because the Profile ID requires 1–32 bytes.
Split the name from the suffix, decode the `u`-prefixed base64url suffix, and compare it with the SHA3-256 of the DER public key.
Reject an ID over 80 bytes and a name containing `@`, an empty name, or a name over 32 bytes.

**Completion criterion**
Tests cover a known `Anonymous@...` ID, an empty display part rejected by the Profile ID layer, a name with an invalid character rejected by the core-rs rule, a tampered suffix, a public key that produces another suffix, and an ID over 80 bytes.

<a id="s-4"></a>
### Step 4. Signed Profile model

**Locations to change**
New `daemon/modules/engine/src/model/profile.rs` and the module export in [model.rs](../../daemon/modules/engine/src/model.rs#L1).

**What to do**
Implement `Profile` with sign and verify around a fixed signing prefix, a `ProfileSigningPayload`, and the `OmniSigner` cert.
Validate the ID against the public key, reject self trust and block entries, reject an ID in both lists, enforce the list and message limits, and check that the encoded body stays within 1 MiB.
Expose the update-time skew check used by the repository.

**Completion criterion**
Tests sign and verify a Profile, reject a changed ID, public key, version, update time, list, message, and signature, reject self and overlapping trust/block entries, and reject an encoded body over 1 MiB.

<a id="s-5"></a>
### Step 5. Profile key store

**Locations to change**
New `daemon/modules/engine/src/core/profile/key.rs` and the module export in `daemon/modules/engine/src/core/profile.rs`.

**What to do**
Create and load an `OmniSigner` at `state_dir/profile/keys/<Profile ID>/signer`.
Write through a temporary file, set `0600` on Unix, rename into place, and verify that the signer display equals the stored ID.
Return an error for a broken key or a name that no longer satisfies the core-rs rule.

**Completion criterion**
Tests cover create-then-load, a restart with the same key and ID, owner-only permissions on Unix, a broken key, and a key whose display does not match the expected ID.

<a id="s-6"></a>
### Step 6. Profile SQLite repository

**Locations to change**
New `daemon/modules/engine/src/core/profile/repo.rs` and the migration list inside `repo.rs`.

**What to do**
Add a SQLite table for the encoded signed body, version, update time, creation time, and active flag, with a partial unique index for one active row.
Implement insert, get, list, active selection, and replace-if-newer.
Reject an update time more than 5 minutes ahead, ignore an older snapshot, use the version on equal times, and keep the first body while logging the second on equal time and version.

**Completion criterion**
Tests cover a restart, one active row, a newer body replacing an older one, an older body being ignored, equal times with a higher version winning, an equal time and version with a different body keeping the first, and a future update time being rejected.

<a id="s-7"></a>
### Step 7. Profile service and startup behavior

**Locations to change**
New `daemon/modules/engine/src/core/profile/service.rs` and the module export in `daemon/modules/engine/src/core/profile.rs`.

**What to do**
Implement `ProfileService` with create, list, get, active, activate, and update through a `ProfileDraft`.
On open, recover each key without a body as an empty Profile, create `Anonymous` when no Profile exists after that, fail on a body without a key, and activate the oldest Profile by creation time and then by Profile ID when none is active.
Reject an update whose current time is earlier than the stored update time; allow an equal time with a higher version.
Serialize updates so two drafts cannot sign from the same snapshot.

**Completion criterion**
Tests cover a fresh state, an existing state without duplication, no active Profile, each key without a body, a body without a key, the `Anonymous` ID, create/list/get/active/activate/update, a restart preserving the active Profile, and a clock that moves back being rejected.

<a id="s-8"></a>
### Step 8. AxusService wiring and status update

**Locations to change**
[service.rs](../../daemon/modules/engine/src/service.rs#L42) for the `AxusService` field and construction, [profile.md §9](../design/profile.md#9-current-status-and-remaining-work), [storage.md §6](../design/storage.md#6-current-status-and-remaining-work), [design.md §6.1](../design.md#61-current-status) and [§6.2](../design.md#62-roadmap), `docs/plans.md`, and this plan.

**What to do**
Construct the Profile service in `AxusService`, expose it through an accessor, and include it in startup failure and shutdown.
Update the status sections to say the Profile foundation is implemented and the ProfileExchanger, Web of Trust, publication integration, and API/UI remain; remove the old "not implemented" and "only the FileRef type exists" text, add the Profile repository and key store to storage.md §6, and note under the roadmap that item 1 is implemented while the publication, API, and UI work remains.
After the acceptance verification passes, fold this plan into the design documents and remove the plan row and file.

**Completion criterion**
An engine test named `profile_axus_service_lifecycle` starts an `AxusService`, reads the active `Anonymous` Profile through the accessor, updates it, restarts the service, sees the updated active Profile, and shuts down; the status sections name the remaining exchange, trust, publication, and API/UI work; `RUSTC_WRAPPER= cargo make test` passes; the branch passes the all-OS `gh workflow run ci.yml --ref feat/profile-foundation`; the plan row and file are removed.

## 6. Acceptance Verification

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_service_startup/)'` | The fresh-state test passes and asserts the `Anonymous` ID, the key file, the stored body, and the active row |
| 2 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_id/)'` | All `ProfileId` tests pass, including empty-name and invalid-character rejection |
| 3 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_service/)'` | Create, list, get, active, activate, update, restart, version+1, update_time=now, signed-body, serialized-update, and clock-back-rejection tests pass |
| 4 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_repo/)'` | The replacement, version, equivocation, and future-time tests pass |
| 5 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_model/)'` | The signature, ID, self, overlap, and limit tests pass |
| 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_service_startup/)'` | The existing-state, no-active, key-without-body, and body-without-key tests pass |
| 7 | `git diff --name-only main...HEAD -- ui-web daemon/entrypoints daemon/openapi.yaml daemon/modules/engine/src/core/negotiator/file daemon/modules/engine/src/core/negotiator/profile_exchanger.rs` | No output; none of these paths changed |
| 7 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/profile_axus_service/)'` | The `profile_axus_service_lifecycle` test passes: it starts an `AxusService`, reads the active Profile through the accessor, updates it, restarts, and shuts down |
| 8 | `cd daemon && RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo make test` | Exit code 0 |
| 8 | `rg -n -e "The design is not implemented" -e "Only the FileRef type exists" docs/design/profile.md` | No matches |
| 8 | `rg -n -e "Profile.*not implemented" -e "not implemented.*Profile" docs/design.md` | No matches |
| 8 | `rg -n "Profile foundation is implemented" docs/design/profile.md docs/design.md` | One match in each file; the surrounding text names the ProfileExchanger, Web of Trust, publication integration, and API/UI as remaining |
| 8 | `rg -n -e "Profile repository" -e "Profile key store" docs/design/storage.md` | At least one match, showing the storage status includes the Profile repository and key store |
| 9 | `run_url=$(gh workflow run ci.yml --ref feat/profile-foundation); head_sha=$(git rev-parse HEAD); if [ -n "$run_url" ]; then run_id=${run_url##*/}; else for i in $(seq 1 30); do run_id=$(gh run list --workflow ci.yml --branch feat/profile-foundation --event workflow_dispatch --limit 20 --json databaseId,headSha --jq "map(select(.headSha == \"$head_sha\"))[0].databaseId // empty"); if [ -n "$run_id" ]; then break; fi; sleep 5; done; fi; gh run watch "$run_id" --exit-status` | Exit code 0; the dispatched run for the current head SHA passes all Ubuntu, macOS, and Windows jobs |

## 7. Risks and Rollback

The core-rs prerequisite is outside this repository.
If the merged core-rs rule changes its mechanism, step 1 and the name tests must be redone, and the plan cannot complete until the submodule points at the merged commit.

The core-rs change narrows both `OmniSigner.name` and `OmniCert.name` from 0–256 to 0–32 bytes; the empty name is allowed.
An existing signer outside the rule can no longer be imported; the daemon is pre-release and no migration is provided.
Reverting the core-rs change requires a separate core-rs revert and another submodule bump.

The Profile ID format is permanent.
It is the trust unit and the input to the ProfileExchanger AssetKey, so changing it after profiles exist breaks every reference.

The key file is the identity root, and the key and SQLite body are not written atomically.
If a crash leaves a key without a body, startup signs an empty Profile; if SQLite loses a body, its lists and messages are lost even though the identity remains.
Deleting `state_dir/profile` loses the identity with no recovery.

A fresh daemon creates `Anonymous` without asking the owner for a name.
The owner can create another Profile and activate it, but the `Anonymous` ID remains in the state.

Reverting the code with `git revert` does not remove `state_dir/profile`.
A rollback that must remove the Profile state needs a separate deletion after the operator confirms the identity is no longer needed.
