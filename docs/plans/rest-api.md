# Plan: REST API for publish and subscribe

## 1. About This Plan

This plan adds the REST API that starts a publish from an HTTP upload, starts a subscription, reports progress, cancels or removes either resource, and serves a completed subscription output.
It follows the layout of the daemon in the sibling Pxna project: the generated interface is implemented by one file per OpenAPI tag, and the daemon keeps its domain processing in a service layer.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| The API decisions, the local-only premise, upload and download placement, and the lifetime of upload state | [daemon-api.md](../design/daemon-api.md#5-design-decisions) |
| Publish, subscription, and block exchange behavior | [file-transfer.md](../design/file-transfer.md#5-block-exchange) and [subscriber state transitions](../design/file-transfer.md#42-subscriber-side) |
| Vocabulary | [terms.md](../terms.md) |
| Bugs fixed separately | [issues.md](../issues.md) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-11-04 against commit `df35197`.

## 2. Purpose and Completion Criteria

A local client can publish a file by uploading it over HTTP, follow the encoding, start a subscription for the returned root hash, follow the fetch and decode, stop or remove either task, and download the completed output, without placing a file on the daemon host by hand.

### 2.1 Completion Criteria

1. `POST /api/v1/publishes` returns `201` with an `Uploading` publish, `PUT /api/v1/publishes/{id}/content?offset=N` accepts chunks, `POST /api/v1/publishes/{id}/commit` returns `202` and starts encoding, and `GET /api/v1/publishes/{id}` reaches `Succeeded` with a root hash.
2. `POST /api/v1/subscribes` returns `202` and starts a subscription for that root hash, `GET /api/v1/subscribes/{id}` reaches `Completed`, and `GET /api/v1/subscribes/{id}/content` returns bytes equal to the uploaded file.
3. A chunk whose `offset` differs from the current written size returns `409`, and `GET /api/v1/publishes/{id}` still reports the previous written size.
4. A commit whose written size differs from the declared size returns `400`, and the publish does not reach `Processing`.
5. `GET /api/v1/subscribes/{id}/content` returns `206` with `Content-Range` for a byte range, returns `304` when `If-None-Match` matches the ETag, returns `409` before `Completed`, and returns `410` when the output file is gone.
6. `POST /api/v1/publishes/{id}/cancel` stops an `Uploading`, `Pending`, `Processing`, or `Failed` publish, and `DELETE /api/v1/publishes/{id}` stops a publication or removes a `Failed` one. `POST /api/v1/subscribes/{id}/cancel` turns `Downloading` and `Decoding` into `Canceled` and leaves `Completed`, `Failed`, and `Canceled` unchanged. `DELETE /api/v1/subscribes/{id}` removes the row and the blocks that lost their last reference; cancel and remove during `Finalizing` return `409`.
7. `GET /api/v1/publishes` and `GET /api/v1/subscribes` honor `limit` and `offset`, return `total`, and reflect a resource created by the same daemon.
8. An upload in progress disappears after a daemon restart and its staging file is removed, an upload idle for 24 hours is removed with its staging file, and a publish that reached `Succeeded` and its root hash remain readable after the restart.
9. A non-loopback `api.listen_addr` logs a warning and the API still starts; a publish whose declared size exceeds `api.max_upload_size` returns `413`.
10. `cargo make lint` and `cargo make test` pass.

### 2.2 Non-goals

| Item | Where it goes |
| --- | --- |
| Authentication and remote access policy | The local-only decision in [daemon-api.md](../design/daemon-api.md#the-rest-api-is-local-only-and-unauthenticated) |
| Separate upload and download resources | Rejected in [daemon-api.md](../design/daemon-api.md#uploads-and-downloads-are-part-of-the-publish-and-subscribe-resources) |
| Persisting the `Uploading` state across restarts | Rejected in [daemon-api.md](../design/daemon-api.md#uploading-state-is-kept-in-memory-until-commit) |
| Publishing a server-side path without an upload | Rejected in [daemon-api.md](../design/daemon-api.md#uploads-and-downloads-are-part-of-the-publish-and-subscribe-resources) |
| Streaming progress over a stream or server-sent events | Rejected in [daemon-api.md](../design/daemon-api.md#long-running-rest-operations-are-domain-resources) |
| Send bandwidth limit | [file-transfer.md §6.2](../design/file-transfer.md#62-deferred) |
| NodeFinder, peer, and transfer statistics APIs | [design.md §2.1](../design.md#21-out-of-scope) |
| The bugs listed in [issues.md](../issues.md) | Fixed after this plan |

## 3. Current State

[openapi.yaml](../../daemon/openapi.yaml#L18) defines only `GET /health`, and the generated interface exposes only the health trait ([health.rs](../../daemon/entrypoints/interface/src/apis/health.rs#L24)).
[ApiServer](../../daemon/entrypoints/daemon/src/server.rs#L16) implements that trait and serves the generated router directly ([server.rs](../../daemon/entrypoints/daemon/src/server.rs#L32)). The daemon has no feature or service layer, and [ApiConfig](../../daemon/entrypoints/daemon/src/config.rs#L60) has only `listen_addr`.

[DaemonState](../../daemon/entrypoints/daemon/src/state.rs#L13) owns `AxusService` privately, and `AxusService` holds `FileExchanger` ([service.rs](../../daemon/modules/engine/src/service.rs#L40)). `FileExchanger` keeps `ExchangeContext` private and exposes it only through a `#[cfg(test)]` accessor ([file_exchanger.rs](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L113)), so no engine facade separates the API from the connection context.

`FilePublisher::import` generates a TSID internally ([file_publisher.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher.rs#L40)). The TaskEncoder opens the source asynchronously ([task_encoder.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L147)), commit deletes the uncommitted row ([store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L246)), and the committed table has no column that points back to the import ID ([repo.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs#L58)). `FileSubscriber::find_file` is available only to tests ([file_subscriber.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber.rs#L64)), although the store query is public ([store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L206)).

Without these changes, a client cannot start or follow a publish or subscription, and a completed publish cannot be found from the ID that started it.

## 4. Change Approach

The OpenAPI contract comes first, because the generated traits decide the daemon-side file boundaries. The daemon is then split into an interface layer per OpenAPI tag and a service layer per domain. The engine gains a file facade and the publisher state needed to keep one ID from upload to committed file. The API layer keeps only the upload state that has no engine row yet, and the two content routes are hand-written because the generated router cannot stream a request or response body.

### 4.1 Assumed Decisions

| Decision | Source |
| --- | --- |
| Long-running operations are domain resources and keep the publish ID after commit | [daemon-api.md](../design/daemon-api.md#long-running-rest-operations-are-domain-resources) |
| API state names map to engine states and hide the engine's `Unknown` state | [daemon-api.md](../design/daemon-api.md#api-state-names-are-a-stable-mapping-of-engine-states) |
| Uploads are under a publish and downloads are under a subscribe | [daemon-api.md](../design/daemon-api.md#uploads-and-downloads-are-part-of-the-publish-and-subscribe-resources) |
| `api.max_upload_size` is in bytes and `0` means unlimited | [daemon-api.md](../design/daemon-api.md#41-configuration-boundaries) |
| The API is local-only and unauthenticated, with a warning for a non-loopback address | [daemon-api.md](../design/daemon-api.md#the-rest-api-is-local-only-and-unauthenticated) |
| The `Uploading` state is kept in memory until commit | [daemon-api.md](../design/daemon-api.md#uploading-state-is-kept-in-memory-until-commit) |
| The engine owns the staged source and deletes it at the end of the publish | [daemon-api.md](../design/daemon-api.md#the-engine-owns-the-staged-source-of-an-uploaded-publish) |
| The maximum block length is 1 MiB | [file-transfer.md](../design/file-transfer.md#set-the-maximum-block-length-to-1-mib) |
| OpenAPI is the source of truth for the REST contract | [daemon-api.md](../design/daemon-api.md#openapi-is-the-source-of-truth-for-the-rest-contract) |

### 4.2 Scope of Impact

Touched: `daemon/openapi.yaml` and its generated interface, `daemon/entrypoints/daemon`, the publisher and subscriber in `daemon/modules/engine/src/core/negotiator/file`, the engine facade in `daemon/modules/engine/src/service.rs`, `daemon/config/axus.toml`, `README.md`, and the design, term, issue, and plan documents.

External changes: new API paths under `/api/v1`, the `api.max_upload_size` configuration key, and additive SQLite columns added by a new named migration.
The API has no existing client other than the health check, so no compatibility breaks.
The P2P wire and the NodeFinder protocol are unchanged.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [1](#s-1) | Split the daemon into interface features and services | | not started |
| [2](#s-2) | Define the API contract and regenerate the interface | 1 | not started |
| [3](#s-3) | Add source ownership, publish ID tracking, and progress to the publisher | 2 | not started |
| [4](#s-4) | Add the FileService facade and public subscriber reads | 3 | not started |
| [5](#s-5) | Implement the publish API and the in-memory upload service | 4 | not started |
| [6](#s-6) | Implement the subscribe API and the content route | 4 | not started |
| [7](#s-7) | Wire configuration, startup sweep, and the loopback warning | 5, 6 | not started |
| [8](#s-8) | Add end-to-end, restart, and error tests | 7 | not started |
| [9](#s-9) | Update the design documents, terms, README, and plan list | 8 | not started |

<a id="s-1"></a>
### Step 1. Split the daemon into interface features and services

**Locations to change**
[server.rs](../../daemon/entrypoints/daemon/src/server.rs#L1) becomes `interface.rs` and `interface/server.rs`; new `interface/features.rs`, `interface/features/health.rs`, `service.rs`, and `service/{publish,subscribe,upload,download}.rs`; [error.rs](../../daemon/entrypoints/daemon/src/error.rs#L1) gains `ApiError`.

**What to do**
Move the health implementation into the health feature file. Create `service.rs` and empty `service/{publish,subscribe,upload,download}.rs` modules so the layout exists; Steps 5 and 6 fill them. Build the outer router in `interface/server.rs` so a hand-written route can be added ahead of the generated router. Add `ApiError` with the mapping from engine error kinds to API statuses, and the per-tag response conversions that the generated traits require.

**Completion criterion**
`cargo check` passes, the health route returns `200` through the new router, and the daemon source tree has one feature file per OpenAPI tag and the four service files named in the locations.

<a id="s-2"></a>
### Step 2. Define the API contract and regenerate the interface

**Locations to change**
[openapi.yaml](../../daemon/openapi.yaml#L18), [gen-interface.sh](../../daemon/entrypoints/gen-interface.sh#L1), the generated `daemon/entrypoints/interface`, and new `daemon/entrypoints/daemon/src/interface/features/publishes.rs` and `features/subscribes.rs`.

**What to do**
Add the `Publishes` and `Subscribes` tags, the paths and status codes of §2.1, and the schemas for publish, subscribe, upload requests, list responses, and errors. Use the API state names of [daemon-api.md](../design/daemon-api.md#api-state-names-are-a-stable-mapping-of-engine-states). Define `GET /subscribes/{id}/content` as a binary response and `PUT /publishes/{id}/content` as a binary request so the generated router owns the paths; the hand-written routes answer them first. Add `unimplemented!()` stubs for the new generated traits in the two feature files, so the generated router mounts and `cargo check` passes; Steps 5 and 6 replace the stubs. Raise the API version to `0.3.0`.

**Completion criterion**
Regenerating the interface from the edited OpenAPI produces the new traits and models, a second regeneration produces no diff, and `cargo check` passes with the temporary stubs in place.

<a id="s-3"></a>
### Step 3. Add source ownership, publish ID tracking, and progress to the publisher

**Locations to change**
[file_publisher.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher.rs#L40), [store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L66), [published_uncommitted_file.rs](../../daemon/modules/engine/src/core/negotiator/file/model/published_uncommitted_file.rs#L1), [published_committed_file.rs](../../daemon/modules/engine/src/core/negotiator/file/model/published_committed_file.rs#L1), [repo.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs#L54), [task_encoder.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L147), and the SQLite migration.

**What to do**
Add the declared size and the source ownership flag to the uncommitted file, and the original publish ID and size to the committed file. Add the columns with a new named migration; `size` and `source_id` are nullable and `source_owned` is `INTEGER NOT NULL DEFAULT 0`, so the old insert statements still work and an old row is never treated as owned ([storage.md §4.7](../design/storage.md#47-migration)). Add an import that takes the API-issued ID, moves the staged file into a publisher-owned source directory, and records the ownership. Delete the source when the publish commits, is cancelled, or fails, and sweep source files without a row at store open. Add the publish read, list, cancel, remove, and encoded block count queries.

**Completion criterion**
Engine tests show that a source is moved, deleted at each terminal state, and swept when orphaned; a committed file can be found by the publish ID; the encoded block count grows while encoding; a store opened from the previous schema keeps working through the new migration; and a row from the previous schema is not treated as owned.

<a id="s-4"></a>
### Step 4. Add the FileService facade and public subscriber reads

**Locations to change**
[service.rs](../../daemon/modules/engine/src/service.rs#L53), new `daemon/modules/engine/src/service/file.rs`, [file_subscriber.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber.rs#L64), and [store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L206).

**What to do**
Add `FileService` as the engine-facing publish and subscribe API, and expose it through `AxusService::file_service()`. Make the subscriber lookup public and add the subscription list query. Keep the publisher and subscriber types out of the daemon layer.

**Completion criterion**
An engine test creates, reads, lists, cancels, and removes a publish and a subscription through `FileService` without naming `FileExchanger` or `ExchangeContext`.

<a id="s-5"></a>
### Step 5. Implement the publish API and the in-memory upload service

**Locations to change**
Fill `daemon/entrypoints/daemon/src/service/upload.rs` and `service/publish.rs` created by Step 1; fill `interface/features/publishes.rs` created by Step 2 and add `interface/features/publishes/route.rs`; [state.rs](../../daemon/entrypoints/daemon/src/state.rs#L13).

**What to do**
Create the API-issued publish ID and keep the staged path, declared size, written size, metadata, and expiry in memory. Implement the chunk append with the offset check and the declared-size check at commit. At commit, pass the staged path and the ID to `FileService`; the engine's import performs the move into its source directory. Implement get, list, cancel, and remove with the status names of [daemon-api.md](../design/daemon-api.md#api-state-names-are-a-stable-mapping-of-engine-states), including the terminal rules of §2.1. A `Failed` publish is removed by cancel or delete and is not retried. Expire idle uploads after 24 hours and remove their staging files.

**Completion criterion**
Router tests show the full upload and commit flow, the `409` for an offset mismatch with an unchanged written size, and the `400` for a size mismatch without encoding.

<a id="s-6"></a>
### Step 6. Implement the subscribe API and the content route

**Locations to change**
Fill `daemon/entrypoints/daemon/src/service/subscribe.rs` and `service/download.rs` created by Step 1; fill `interface/features/subscribes.rs` created by Step 2 and add `interface/features/subscribes/route.rs`.

**What to do**
Implement subscribe create with `202`, get, list, cancel, and remove with the API state mapping of [daemon-api.md](../design/daemon-api.md#api-state-names-are-a-stable-mapping-of-engine-states). Cancel turns `Downloading` and `Decoding` into `Canceled` and leaves `Completed`, `Failed`, and `Canceled` unchanged, following [the subscriber transitions](../design/file-transfer.md#42-subscriber-side). Implement the content route with `Range`, `Content-Range`, `Accept-Ranges`, `ETag`, and `If-None-Match`, reading the completed output from the subscriber. Return `409` before `Completed` and `410` when the output file no longer exists.

**Completion criterion**
Router tests show a completed subscription returns the expected bytes, a byte range returns `206` with the correct `Content-Range`, and `If-None-Match` returns `304`.

<a id="s-7"></a>
### Step 7. Wire configuration, startup sweep, and the loopback warning

**Locations to change**
[config.rs](../../daemon/entrypoints/daemon/src/config.rs#L60), [state.rs](../../daemon/entrypoints/daemon/src/state.rs#L13), [executor.rs](../../daemon/entrypoints/daemon/src/executor.rs#L1), [axus.toml](../../daemon/config/axus.toml#L1), and [README.md](../../README.md).

**What to do**
Add `api.max_upload_size` in bytes with the default `0`, meaning unlimited. Pass it and `core.state_dir/uploads` to the upload service. Remove the previous startup's upload staging before the API starts. Log a warning when the API listen address is not loopback.

**Completion criterion**
The configuration tests show the new key and its `0` default, a declared size over the limit returns `413`, and a non-loopback address starts the daemon with a warning.

<a id="s-8"></a>
### Step 8. Add end-to-end, restart, and error tests

**Locations to change**
New `daemon/entrypoints/daemon/src/interface/rest_api_tests.rs`, the `daemon/entrypoints/daemon/src/interface/server.rs` created by Step 1, and [test_support.rs](../../daemon/modules/engine/src/test_support.rs#L1).

**What to do**
Cover the whole path over the router: upload, publish, subscribe, download, list pagination, cancel, and remove. Add restart tests for an in-progress upload and a completed publish, a clock-driven test for the 24-hour upload expiry, and error tests for the status codes of §2.1.

**Completion criterion**
The new tests pass together with the existing engine tests, the restart test observes the staged upload removed and the committed root hash still readable, and the expiry test removes an idle upload without waiting 24 hours.

<a id="s-9"></a>
### Step 9. Update the design documents, terms, README, and plan list

**Locations to change**
[daemon-api.md](../design/daemon-api.md#6-current-status-and-remaining-work), [file-transfer.md](../design/file-transfer.md#7-current-status-and-remaining-work), [design.md](../design.md#6-current-status-and-remaining-work), [issues.md](../issues.md#3-not-included-here), [terms.md](../terms.md), [README.md](../../README.md), and [plans.md](../plans.md).

**What to do**
Move the implemented API into the current-status sections, remove the API item from the roadmap, and renumber the remaining roadmap rows. Remove the REST API rows from the not-included list. Add the publish, subscribe, publish ID, `Uploading`, and `api.max_upload_size` terms to the glossary. Document the new configuration key. Advance this plan's row and remove it when the acceptance verification passes.

**Completion criterion**
The design documents describe the API in the present tense, the glossary contains the new API terms, the README documents `api.max_upload_size`, the removed issue and roadmap rows no longer point at deleted headings, and every document link resolves.

## 6. Acceptance Verification

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1, 2 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api/)'` | The `201` create, `202` commit and subscribe, upload, publish, subscribe, and download flow passes |
| 3, 4 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api/)'` | The offset and size mismatch tests pass |
| 5 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api/)'` | The range, ETag, not-ready, and gone tests pass |
| 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api/)'` | The cancel and remove tests pass, including the terminal-state cancel no-op |
| 7 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api/)'` | The list and pagination tests pass |
| 8 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api/)'` | The restart and 24-hour expiry tests pass |
| 9 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/rest_api|config/)'` | The configuration and warning tests pass |
| 10 | `cd daemon && RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo make test` | Exit code 0 |

The new tests guard the upload offset and size rules, the source cleanup, the publish ID after commit, the download range and ETag, the cancel and remove rules, and the restart behavior.

## 7. Risks and Rollback

| Change | Failure and rollback |
| --- | --- |
| Additive SQLite columns in the publisher store | The columns are added by a new named migration ([storage.md §4.7](../design/storage.md#47-migration)); `size` and `source_id` are nullable and `source_owned` is `INTEGER NOT NULL DEFAULT 0`, so the old insert statements still work after a code revert and an old row is not treated as owned. The columns stay in place and no data is lost. |
| Source deletion at commit, cancel, and failure | An uploaded source is gone after a failed publish and must be uploaded again. This is not reversible. |
| Startup removal of the upload staging directory | An upload in progress during a crash is lost. The client must create a new publish and upload again. This is not reversible. |
| New API under `/api/v1` | No existing client uses the new paths, so a revert only removes the endpoints. |
