# Plan: the core transfer REST API

## 1. About this plan

This plan implements the publication and subscription resources of [daemon-api.md](../design/daemon-api.md#5-rest-resources), with staged uploads, the download directory, and the static serving of a build output.
It does not add authentication, library roots, or the browser client; those are later items of the roadmap.

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | The order, the steps, and the acceptance checks of this work |
| [daemon-api.md](../design/daemon-api.md#5-rest-resources) | The REST resources, the authentication design, the reach, and the path rules |
| [file-transfer.md](../design/file-transfer.md#41-publisher-side) | The publisher and subscriber state machines |
| [storage.md](../design/storage.md#43-publishing-commit-and-reclamation) | The staged area, the output placement, the publication mapping, and recovery |
| [openapi.yaml](../../daemon/openapi.yaml) | The REST paths, schemas, and status codes |
| [terms.md](../terms.md) | The vocabulary |

### 1.2 Creation date and target commit

Created on 2026-11-06 against `60ce382` on `main`.

## 2. Purpose and completion conditions

A client can publish a file by declaring its size, uploading it in ranges, and polling until the file is committed, and can subscribe to a root hash and download the output under the configured directory.
The end-to-end path from an upload on one daemon to a download on another is confirmed through the REST API.

### 2.1 Completion conditions

1. `POST /publications` with a name and a size returns `202` and a publication in `Receiving`.
2. `PUT /publications/{id}/content` with `Content-Range` appends bytes; a gap or an overlap returns `409` with the current `receivedBytes`.
3. The last range moves the publication to `Pending`, and the publisher commits it; polling observes `Committed` with the root hash.
4. A daemon restart between two ranges keeps `receivedBytes`, and the next range continues from it.
5. `POST /subscriptions` with the root hash and the name returns `202`; polling observes `Completed`; `GET /subscriptions/{id}/content` returns the output with `Range` and `ETag`.
6. The output file appears under `storage.download_dir`, and the staged file disappears from `state_dir/uploads` after the commit.
7. `POST /{id}/cancel` leaves a `Canceled` row, and `DELETE /{id}` removes it; `DELETE` of a committed publication removes the committed file and every publication ID of the pair.
8. An unknown path under `/api` returns `404`, and an unknown path under `web.dir` returns `index.html`.
9. `api.listen_addr` outside loopback fails startup with an error that names the missing authentication.
10. `cargo make lint` passes.

### 2.2 Non-goals

- Owner registration and sessions; the API stays loopback-only.
- Library roots and their listing.
- The browser client and its build.
- Streaming progress.
- Changes to the P2P wire format.

## 3. Current state

The daemon serves only `/health`, and `openapi.yaml` declares only that path.
`FilePublisher::import` takes a complete file and creates a `Pending` row; there is no way to create a row before its bytes exist.
`PublishedUncommittedFile` has no size, received offset, or `Receiving`/`Canceled` state.
`FileSubscriber::subscribe` takes a destination path from the caller.
`config.rs` has no `storage.download_dir`, `web.dir`, or `api.max_upload_size`.
The API listen address accepts any address.

## 4. Approach

The publication row becomes the identity of the whole import.
`POST /publications` allocates the ID and inserts a `Receiving` row whose `file_path` is the staging path.
The content route writes to the staging file, advances `received_bytes` after each write, and moves the row to `Pending` when the size is reached.
The encoder skips `Receiving`, and the commit writes the publication mapping in the same SQLite transaction as the committed file.

### 4.1 Decisions this plan depends on

| Decision | Source |
| --- | --- |
| Publications and subscriptions are mirrored resources | [daemon-api.md §9.1](../design/daemon-api.md#91-decided) |
| Content is a sub-resource and the last byte commits | [daemon-api.md §9.1](../design/daemon-api.md#91-decided) |
| A publication row exists from declaration | [file-transfer.md §6.1](../design/file-transfer.md#61-decided) |
| The output goes to `storage.download_dir` | [storage.md §5.1](../design/storage.md#51-decided) |
| The API stays loopback-only until the owner session exists | [design.md §6.2](../design.md#62-roadmap) |

### 4.2 Impact

- `openapi.yaml` and the generated interface gain the new paths and schemas.
- `config.rs` gains `storage.download_dir`, `web.dir`, and `api.max_upload_size`; existing configuration files keep working because all three default.
- The publisher SQLite gains `size`, `received_bytes`, `source_owned`, and the `publication_records` table, added by a named migration.
- The staged upload area is a new directory under `state_dir`; an interrupted upload survives a restart.
- No P2P wire format changes.

## 5. Steps

| # | Step | Depends on | State |
| --- | --- | --- | --- |
| 1 | Extend the publisher model, repository, and migration | - | Complete |
| 2 | Add create, append, complete, cancel, and remove to the publisher | 1 | Complete |
| 3 | Expose the publication and subscription operations through `AxusService` | 2 | Complete |
| 4 | Add the configuration settings and the loopback guard | - | Complete |
| 5 | Declare the contract in `openapi.yaml` and regenerate the interface | - | Complete |
| 6 | Implement the publication handlers | 2, 5 | In progress |
| 7 | Implement the subscription handlers and static serving | 3, 5 | In progress |
| 8 | Add integration tests through the router | 6, 7 | Not started |
| 9 | Update the README, the sample configuration, and the design status | 6, 7 | In progress |

<a id="s-1"></a>
### Step 1. Extend the publisher model, repository, and migration

**Changed locations**
`daemon/modules/engine/src/core/negotiator/file/model/published_uncommitted_file.rs`, `file_publisher/repo.rs`.

**What to do**
Add `Receiving` and `Canceled` to the state enum, and `size`, `received_bytes`, and `source_owned` to the row.
Add a `publication_records` table that maps a publication ID to the committed file's root hash and name.
Apply both additions as one named migration; existing rows read as `received_bytes = 0` and `size = NULL`.

**Completion condition**
A round-trip test stores and reads a `Receiving` row and a mapping row, and reopening the repository keeps both.

<a id="s-2"></a>
### Step 2. Add create, append, complete, cancel, and remove to the publisher

**Changed locations**
`file_publisher/store.rs`, `file_publisher.rs`, `task_encoder.rs`.

**What to do**
Add `create_publication(id, staging_path, name, size, block_size)` and `append_content(id, offset, bytes)`.
`append_content` rejects an offset other than `received_bytes` with a conflict that carries the current value, writes and syncs the bytes, advances `received_bytes`, and moves the row to `Pending` when it reaches `size`.
Make `cancel` set `Canceled` and keep the row, and `remove` delete the row and its staged file.
Make the encoder claim only `Pending` and delete the staged file after the commit.
Write the publication mapping in the commit transaction.

**Completion condition**
A publisher test creates a 2 MiB publication, appends it in two ranges, commits it, and reads the mapping; a mismatched offset leaves the row and the file unchanged.

<a id="s-3"></a>
### Step 3. Expose the publication and subscription operations through AxusService

**Changed locations**
`daemon/modules/engine/src/service.rs`, `model.rs`.

**What to do**
Add the publication operations and the subscription list/find operations the API needs, and a `Publication` view that carries the API state, size, received bytes, and root hash.
Keep the engine's existing subscription entry points and adapt the destination at the daemon layer.

**Completion condition**
A service test creates a publication, reads it, lists it, cancels it, and removes it, and the same for a subscription.

<a id="s-4"></a>
### Step 4. Add the configuration settings and the loopback guard

**Changed locations**
`daemon/entrypoints/daemon/src/config.rs`, `executor.rs`.

**What to do**
Add `storage.download_dir` (default `state_dir/downloads`), `web.dir`, and `api.max_upload_size` (default `0`, unlimited).
Reject a non-loopback `api.listen_addr` at startup with a message that names the missing authentication.

**Completion condition**
A configuration test accepts the defaults and a loopback address, and rejects a non-loopback address.

<a id="s-5"></a>
### Step 5. Declare the contract and regenerate the interface

**Changed locations**
`daemon/openapi.yaml`, `daemon/entrypoints/gen-interface.sh`, `daemon/entrypoints/interface`.

**What to do**
Declare the publication and subscription paths, the content routes, the errors, and the schemas.
Regenerate the interface twice and confirm no diff.

**Completion condition**
The generated router exposes every declared route, and a second generation produces no diff.

<a id="s-6"></a>
### Step 6. Implement the publication handlers

**Changed locations**
`daemon/entrypoints/daemon/src/server.rs` and a new `server/` module.

**What to do**
Create, list, read, cancel, and delete publications; stream content into the staging file with the offset check; map engine errors to `409`, `413`, `507`, and `404`.
The byte route is handwritten in front of the generated router.

**Completion condition**
The router tests in step 8 pass for publications.

<a id="s-7"></a>
### Step 7. Implement the subscription handlers and static serving

**Changed locations**
`server.rs`, a new `server/` module.

**What to do**
Create, list, read, cancel, and delete subscriptions; serve the output with `Range`, `ETag`, and `Content-Disposition`.
Serve `web.dir` with the `index.html` fallback and the `/api` 404 rule.

**Completion condition**
The router tests in step 8 pass for subscriptions and static serving.

<a id="s-8"></a>
### Step 8. Add integration tests through the router

**Changed locations**
the daemon's test module.

**What to do**
Start two daemons, upload on one, subscribe on the other, download, and compare the bytes.
Cover an offset mismatch, a restart between ranges, cancel and delete, and the static fallback.

**Completion condition**
The tests pass in `cargo make test`.

<a id="s-9"></a>
### Step 9. Update the README, the sample configuration, and the design status

**Changed locations**
`README.md`, `daemon/config/axus.toml`, `docs/design/daemon-api.md` §10, `docs/design/storage.md` §6, `docs/design/file-transfer.md` §7.

**What to do**
Document the new settings and the loopback restriction, and move the implemented parts out of the remaining work.

**Completion condition**
The README tables name the new keys, and the status chapters list only what remains.

## 6. Acceptance verification

| Condition | Command | Expected result |
| --- | --- | --- |
| 1, 2, 3 | `cd daemon && RUSTC_WRAPPER= cargo nextest run -p omnius-axus-daemon -E 'test(rest_api)'` | The upload flow test passes, including the `409` and the final `Committed` |
| 4 | the same command | The restart test passes and the second range is accepted |
| 5, 6, 7 | the same command | The download test passes and the files are at the configured paths |
| 8 | the same command | The static and API fallback tests pass |
| 9 | `cd daemon && RUSTC_WRAPPER= cargo nextest run -p omnius-axus-daemon -E 'test(config)'` | The non-loopback test passes |
| 10 | `cd daemon && RUSTC_WRAPPER= cargo make lint` | No warnings or errors |
| 1-9 | `cd daemon && RUSTC_WRAPPER= cargo make test` | All tests pass |

## 7. Risks and rollback

- **Schema migration.** The publisher SQLite gains columns and a table.
  The migration is additive and named, so older code ignores the new columns; a rollback keeps the data and only stops using it.
- **Staged uploads.** A staged file without a `Receiving` row is deleted at startup.
  If the reconciliation is wrong, an upload restarts from a lower offset, which the client handles; no committed data is affected.
- **Cancellation semantics.** Cancel now keeps a row instead of deleting it.
  The sweep condition must keep the blocks of `Receiving`, `Pending`, and `Processing` rows only; a mistake here deletes blocks of an active import, which the re-import handles but the acceptance tests must catch.
- **API surface without authentication.** The loopback guard is the only protection until the owner session lands.
  The guard must reject a non-loopback address before the listener binds.
