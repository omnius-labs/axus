# Plan: REST API for publish, subscribe, progress, and cancel

## 1. About This Plan

This plan adds the REST API that a browser client uses to publish and subscribe to files, and the engine changes that API needs.
It first fixes the three Medium bugs of [issues.md](../issues.md), since each of them affects operating the daemon through this API.

### 1.1 Document Responsibilities

| Item | Owner |
| --- | --- |
| Steps, completion criteria, and acceptance verification of this work | This document |
| The contract, reach, uploads, and downloads, and the reasons for the decisions | [daemon-api.md §4.3](../design/daemon-api.md#43-rest-api) and [§5.1](../design/daemon-api.md#51-decided) |
| Import IDs and the deletion of an output | [file-transfer.md §6.1](../design/file-transfer.md#61-decided) |
| Directories under `state_dir` and output operations | [storage.md §3.1](../design/storage.md#31-structure-of-state_dir) and [§4.5](../design/storage.md#45-output-finalization-and-recovery) |
| Paths, schemas, and status codes | [openapi.yaml](../../daemon/openapi.yaml#L18) |
| Vocabulary | [terms.md](../terms.md) |
| Bugs fixed here | [issues.md](../issues.md) |

The family map is in [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation Date and Target Commit

Created on 2026-11-05 against commit `df35197`.

## 2. Purpose and Completion Criteria

From a browser on a host that reaches the daemon, a user uploads a file and obtains its root hash, subscribes to a root hash and watches its progress, cancels the work, and downloads the received file.
Completing it moves the browser client to the front of [design.md §6.2](../design.md#62-roadmap).

### 2.1 Completion Criteria

1. With the destination directory gone, `remove` of a Failed or Canceled subscription succeeds and the sweep stops retrying.
2. After a daemon holding `axus.lock` is killed, the next start acquires the lock with the file still present, and a start while another process holds it fails.
3. While one subscription writes its temporary output, another subscription with a different destination reaches Completed.
4. `POST /publications` with a 32 MiB body returns 202, a larger body returns 413, and polling the returned ID reports Pending or Processing, then Committed with a root hash, and Failed with a reason for a failed encode; the root hash stays readable after the same content is uploaded again and after a restart.
5. An upload session rejects an offset mismatch with 409, a chunk over 32 MiB with 400, and a commit of missing bytes with 400, resumes from the offset `GET /uploads/{id}` reports, expires after 24 hours with its bytes deleted, and its commit returns a publication.
6. `DELETE /publications/{id}` cancels an uncommitted file, stops a committed one so its root hash is no longer provided unless another committed file has the same root hash, and leaves no staged file in either case; a staged file no row points at is deleted at startup.
7. `POST /subscriptions` places the output in the download directory, returns 400 for a name that is not one path component, and returns 409 for a name in use.
8. Two daemons wired through the API transfer a file: the subscription reports its status, rank, and downloaded and total block counts, reaches Completed, and `GET /subscriptions/{id}/content` returns the same bytes, including a Range request, as an `application/octet-stream` attachment.
9. `POST /subscriptions/{id}/cancel` turns a subscription into Canceled, cancel and delete return 409 while finalizing, and `DELETE /subscriptions/{id}` keeps the output unless `deleteOutput=true`, and keeps it when another row reaches the same entry.
10. A request whose `Origin` differs from `Host` returns 403 on every path, one without `Origin` passes, a non-loopback `api.listen_addr` starts and logs a warning, and with `web.dir` set a path outside `/api` returns a file or `index.html`, a path with a `..` component never returns a file above `web.dir`, and an unknown `/api` path returns the API's 404.
11. The format, clippy, and style checks report no violation, and every test passes.

### 2.2 Non-goals

| Item | Where it goes |
| --- | --- |
| The browser client itself | Item 1 of [design.md §6.2](../design.md#62-roadmap) |
| Server-sent events and an encoding progress ratio | Rejected for now in [daemon-api.md §5.1](../design/daemon-api.md#51-decided), addable as optional fields |
| Choosing the block size, attributes, or priority through the API | [daemon-api.md §5.1](../design/daemon-api.md#hide-the-block-size-attributes-and-priority-from-the-api) |
| An API for node information, and a share link format | Deferred in [daemon-api.md §5.2](../design/daemon-api.md#node-information-and-share-links-in-the-rest-api) |
| The two Low bugs of [issues.md](../issues.md) | They stay listed there |

## 3. Current State

[openapi.yaml](../../daemon/openapi.yaml#L18) defines only `/health`, and [ApiServer](../../daemon/entrypoints/daemon/src/server.rs#L32) serves only the generated router.
[AxusService](../../daemon/modules/engine/src/service.rs#L38) exposes only `new` and `shutdown`, and [FileExchanger](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L44) holds FilePublisher and FileSubscriber in a private context.
[FilePublisherStore::import](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L66) reads a host path that it never deletes, and [commit_file_with_blocks](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs#L314) deletes the row of the temporary ID, leaving nothing that maps it to the root hash.
[FileSubscriberStore::subscribe](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L91) takes a host path, [remove](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L185) never deletes an output, and a violated output reservation surfaces as a database error rather than `AlreadyExists`.
[delete_temporary_output](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L413) fails when the destination directory is gone, one [output_lock](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L38) serves all subscriptions, and [FileLock::acquire](../../daemon/refs/core-rs/modules/base/src/file_lock.rs#L23) judges only by the file's existence.

Without these changes, no client can start a publish or a subscription, and one that could would never learn the root hash of its publish.

## 4. Change Approach

The bugs are fixed first because they change the Store code the later steps build on.
The engine then gains the publication record, staged sources, listing, and output deletion, exposed through AxusService.
On top, the daemon gains the contract, the common request handling, and the routes for publications, uploads, and subscriptions.

### 4.1 Assumed Decisions

| Decision | Source |
| --- | --- |
| Poll durable resources | [daemon-api.md](../design/daemon-api.md#poll-durable-resources-for-long-running-operations) |
| Any listen address, no authentication | [daemon-api.md](../design/daemon-api.md#listen-on-any-address-without-authentication) |
| `Origin` compared with `Host`, and no `Host` check | [daemon-api.md](../design/daemon-api.md#reject-a-cross-origin-request-and-do-not-check-the-host) |
| Same-origin static serving | [daemon-api.md](../design/daemon-api.md#serve-the-browser-client-from-the-apis-origin) |
| One request up to 32 MiB, and a session above | [daemon-api.md](../design/daemon-api.md#upload-in-one-request-up-to-32-mib-and-through-a-session-above-that) |
| An output served as an opaque attachment | [daemon-api.md](../design/daemon-api.md#serve-an-output-as-an-opaque-attachment) |
| Handwritten routes for file bytes | [daemon-api.md](../design/daemon-api.md#let-handwritten-routes-carry-file-bytes) |
| Import IDs stay readable | [file-transfer.md](../design/file-transfer.md#keep-the-import-id-of-a-committed-file-readable) |
| remove deletes an output on request | [file-transfer.md](../design/file-transfer.md#let-remove-delete-an-output-only-on-request) |

### 4.2 Scope of Impact

Touched: `core/negotiator/file`, `service.rs`, `model.rs`, `entrypoints/daemon`, `entrypoints/interface` (generated), `openapi.yaml`, and `FileLock` in core-rs.
Externally visible: new REST paths, the optional `storage.download_dir` and `web.dir` settings, new directories under `state_dir`, and a new table in the publisher SQLite.
Nothing breaks compatibility: `/health` and existing settings keep their meaning, and the migration only adds a table.
`FileLock` no longer deletes its file on release, and `FileLock` has no user other than this daemon.
FileExchanger's wire and NodeFinder are not touched.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [1](#s-1) | Treat a vanished destination as a deleted temporary output | | done |
| [2](#s-2) | Hold `axus.lock` with an OS advisory lock | | done |
| [3](#s-3) | Serialize output operations per destination | 1 | done |
| [4](#s-4) | Publication record and staged sources | | done |
| [5](#s-5) | Subscription listing, conflict, and output deletion | 3 | done |
| [6](#s-6) | AxusService operations | 4, 5 | done |
| [7](#s-7) | Contract, configuration, and common request handling | 2, 6 | done |
| [8](#s-8) | Publication and upload routes | 7 | done |
| [9](#s-9) | Subscription and content routes | 7 | done |
| [10](#s-10) | Close-out of the documents | 8, 9 | done |

<a id="s-1"></a>
### Step 1. Treat a vanished destination as a deleted temporary output

**Locations to change**
[store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L413) and the issue `vanished-output-directory-blocks-sweep-and-remove` in [issues.md](../issues.md).

**What to do**
For Canceled and Failed rows, count a missing destination directory as the temporary output being gone, and keep the check for the path that writes a new temporary output.
Remove the issue's child document and row.

**Completion criterion**
Tests show `remove` succeeds with the destination directory gone and the sweep leaves no retry pending.

<a id="s-2"></a>
### Step 2. Hold `axus.lock` with an OS advisory lock

**Locations to change**
[file_lock.rs](../../daemon/refs/core-rs/modules/base/src/file_lock.rs#L23) in core-rs, the core-rs submodule pointer, and the issue `daemon-lock-survives-abnormal-exit` in [issues.md](../issues.md).

**What to do**
Acquire with `File::try_lock`, report a held lock as `AlreadyExists`, and leave the file in place on release, because deleting it lets a process lock an unlinked file while another locks a new one.
Remove the issue's child document and row.

**Completion criterion**
Tests show acquisition fails while another process holds the lock and succeeds with the file present after that process is killed.

<a id="s-3"></a>
### Step 3. Serialize output operations per destination

**Locations to change**
[store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L38) and the issue `output-lock-shared-across-subscriptions` in [issues.md](../issues.md).

**What to do**
Replace `output_lock` with a lock per output path whose entries disappear when unused, held by `OutputWriter`, `finalize`, `discard_output`, and the sweep.
Remove the issue's child document and row.

**Completion criterion**
A test shows a held writer does not block `finalize` of another destination, and the existing Store tests pass.

<a id="s-4"></a>
### Step 4. Publication record and staged sources

**Locations to change**
[file_publisher/repo.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs#L314), [file_publisher/store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L66), [task_encoder.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L140), and [file_publisher.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher.rs#L40).
New: `core/negotiator/file/model/publication.rs` holds the publication type that the API reads.

**What to do**
Add a migration with the import record table, written in the commit transactions and removed with the committed file.
Add an import of a staged file that moves it under `staged/`, its deletion after commit, cancel, and failure, and its reclamation at startup.
Add listing, lookup, and deletion by import ID.

**Completion criterion**
Tests show the import ID reads Committed with its root hash after the commit, after a repeated import, and after a reopen; deletion reaches both an uncommitted and a committed row; and no staged file remains after commit, cancel, failure, or a reopen with an orphan.

<a id="s-5"></a>
### Step 5. Subscription listing, conflict, and output deletion

**Locations to change**
[file_subscriber/store.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L91) and [file_subscriber.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber.rs#L51).

**What to do**
Add listing, return `AlreadyExists` for a violated output reservation, and add a removal that, only when asked, also deletes a Completed output unless another row reaches the same entry.

**Completion criterion**
Tests show the error kind of a second subscription to an in-use destination; that an unrequested removal keeps the output; and that a requested one deletes it, or keeps it when a second row is at the same destination.

<a id="s-6"></a>
### Step 6. AxusService operations

**Locations to change**
[service.rs](../../daemon/modules/engine/src/service.rs#L38), [file_exchanger.rs](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L113), and [model.rs](../../daemon/modules/engine/src/model.rs#L9).

**What to do**
Expose the operations of steps 4 and 5 on AxusService, along with the model types they return.

**Completion criterion**
A test with two wired nodes imports a staged file through AxusService, subscribes to its root hash, and reads a Completed output with the same bytes.

<a id="s-7"></a>
### Step 7. Contract, configuration, and common request handling

**Locations to change**
[openapi.yaml](../../daemon/openapi.yaml#L18), [the generated router](../../daemon/entrypoints/interface/src/server/mod.rs#L21), [config.rs](../../daemon/entrypoints/daemon/src/config.rs#L23), [state.rs](../../daemon/entrypoints/daemon/src/state.rs#L13), and [server.rs](../../daemon/entrypoints/daemon/src/server.rs#L32).
New under `entrypoints/daemon/src/server/`: `origin_check.rs` for the `Origin` rule, `web_assets.rs` for static serving, and `api_error.rs` for converting engine errors.

**What to do**
Define every path and regenerate the interface.
Add `storage.download_dir` and `web.dir`, the startup warning, and the conversion of engine errors to the table in [daemon-api.md §4.3](../design/daemon-api.md#43-rest-api).

**Completion criterion**
Tests show the `Origin` rule, static serving with the fallback, a `..` path staying inside `web.dir`, the API's 404, and the configuration defaults; `/health` still answers.

<a id="s-8"></a>
### Step 8. Publication and upload routes

**Locations to change**
New under `entrypoints/daemon/src/server/`: `publications.rs` for the publication routes, `uploads.rs` for the upload routes, and `upload_session.rs` for `UploadSession` and its expiry.

**What to do**
Stream bodies to `<state_dir>/uploads/`, apply the limits, and hand a finished file to the engine.
Serve the rest of the publication paths and session lifecycle through the generated traits.

**Completion criterion**
Router tests show each status of criteria 4, 5, and 6.

<a id="s-9"></a>
### Step 9. Subscription and content routes

**Locations to change**
New under `entrypoints/daemon/src/server/`: `subscriptions.rs` for the subscription routes and the content route, and `file_name.rs` for the validation of a file name.

**What to do**
Validate the name, place outputs in the download directory, and serve the content with the headers of [daemon-api.md §5.1](../design/daemon-api.md#serve-an-output-as-an-opaque-attachment).

**Completion criterion**
Router tests show each status of criteria 7 and 9, and the content headers and Range response of criterion 8.

<a id="s-10"></a>
### Step 10. Close-out of the documents

**Locations to change**
[design.md](../design.md#21-out-of-scope), [daemon-api.md](../design/daemon-api.md#6-current-status-and-remaining-work), [session.md](../design/session.md), [issues.md](../issues.md#3-not-included-here), and [terms.md](../terms.md).

**What to do**
Update the scope, the structure diagram, the current status, and the roadmap, delete the row of [issues.md](../issues.md#3-not-included-here) that says the REST API is missing, then confirm the terms match the code.

**Completion criterion**
No document says the REST API for publishing is missing, the roadmap's next item is the browser client, and every path and setting this plan adds that [terms.md](../terms.md) names matches `openapi.yaml` and `config.rs`.

## 6. Acceptance Verification

| Criterion | Command | Expected result |
| --- | --- | --- |
| 1 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/vanished/)'` | All pass |
| 2 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'package(omnius-core-base) & test(/file_lock/)'` | All pass, including the killed holder test |
| 3 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/other_destination/)'` | All pass |
| 4 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/server::publications/) + test(/file_publisher::(store|repo)::tests::(import_id|the_import_migration)/)'` | All pass |
| 5 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/server::uploads|server::upload_session/)'` | All pass |
| 6 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/server::publications/) + test(/file_publisher::store::tests::staged/)'` | All pass |
| 7 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/server::subscriptions|server::file_name/)'` | All pass |
| 8 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/two_daemons/)'` | All pass, and the test compares the downloaded bytes with the uploaded ones |
| 9 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/server::subscriptions/) + test(/file_subscriber::store::tests::remove_output/)'` | All pass |
| 10 | `cd daemon && RUSTC_WRAPPER= cargo nextest run --all-features -E 'test(/server::origin_check|server::web_assets|config::tests/)'` | All pass |
| 11 | `cd daemon && RUSTC_WRAPPER= cargo make lint && TESTCONTAINERS_HOST_OVERRIDE=<docker host> RUSTC_WRAPPER= cargo nextest run --all-features --no-fail-fast` | Exit code 0. The override is needed only when the Docker daemon publishes ports outside the test process's loopback |

New tests protect the following: the import record keeps a root hash reachable from its import ID; the staged tests keep uploads from leaking disk; the origin test keeps another site from driving the API; the content test keeps an output from rendering in the API's origin.

## 7. Risks and Rollback

The migration adds a table to the publisher SQLite, and `git revert` does not remove it.
The older code ignores an unknown table, so rolling back the code is safe and the table can stay.
Once a client uses the new paths, withdrawing them breaks that client, so a revert needs the client changed with it; before the browser client exists no client depends on them.
An output deleted through `deleteOutput=true` cannot be restored by any rollback; subscribing to its root hash again fetches it while another node still provides it.
If the core-rs change is reverted, the submodule pointer moves back and the lock reverts to judging by existence, with a leftover `axus.lock` then blocking the start until it is deleted by hand.
