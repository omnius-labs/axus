# Design of the browser client

## 1. About this document

This document covers the screens, the routes, the polling, the upload flow, and the build of the browser client that operates the daemon.
The main readers are developers who change `ui-web/` and AI agents that carry the decisions forward.
This document assumes the vocabulary of [terms.md](../terms.md#2-terminology). Term definitions are not given here.

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure and boundaries between concerns |
| This document | Screens, routes, polling, the client's upload and download flow, the build |
| [daemon-api.md](./daemon-api.md#1-about-this-document) | REST resources, authentication, static serving, reach, and the path rules |
| [openapi.yaml](../../daemon/openapi.yaml) | REST API paths, schemas, and status codes |
| `ui-web/` | The client's implementation |
| [README.md](../../README.md) | Setup and development procedures |

### 1.2 Tenses used in this document

The body describes the completed design, in the present tense.
The implementation status and the remaining work are consolidated in §7.
To check what runs now, read §7 first.

## 2. Responsibilities and boundaries

The client renders the daemon's resources, starts transfers, and reports their progress.
It holds no transfer logic of its own: the daemon owns the bytes, the paths, and the states, and the client only chooses among the identifiers the daemon issued.

The client is served from the daemon's origin ([daemon-api.md §7](./daemon-api.md#7-serving-the-browser-client)) and carries no independent deployment.
It does not read or write the host's filesystem, does not construct a path, and does not guess a media type for an output.

## 3. Screens and routes

| Route | Screen |
| --- | --- |
| `/setup` | First-run registration |
| `/login` | Login |
| `/publications` | Publications, with the entry to a new publication |
| `/publications/new` | Choose an upload or a library file, then upload |
| `/publications/{id}` | Publication detail, progress, cancel, delete, share text |
| `/subscriptions` | Subscriptions, with the form to start one |
| `/subscriptions/{id}` | Subscription detail, progress, cancel, delete, download |

The routes are the client's own. The resources each screen uses are described in §3.1 to §3.4, and their authoritative paths are declared in [openapi.yaml](../../daemon/openapi.yaml).
The literal segment `new` takes precedence over an ID, and a generated publication ID never equals it.

### 3.1 Setup and login

`/setup` is reachable only while no owner exists; after the owner exists, it redirects to `/login`.
It asks for the owner name and a password twice and then moves to the publication list.
`/login` asks for the owner name and the password, and any other route redirects there with the requested route kept for the return.

The client never stores a credential in web storage.
The session travels as an `HttpOnly` cookie, so a script on the page cannot read it.

### 3.2 Publications

The list shows one row per committed file and per active publication, with the name, the state, the progress while receiving, the root hash when committed, and the time.
The row of a committed publication offers the share text: the root hash and the name, which the recipient enters in their own node.
A repeated import of the same content under the same name appears once, under the oldest ID, and the detail of any ID stays reachable.

The detail shows the same fields, the reason of a failure, and the operations the state allows.
Cancel and delete follow [daemon-api.md §5.5](./daemon-api.md#55-cancellation-and-deletion); the client does not offer a button whose answer would be `409`.

### 3.3 Subscriptions

The list shows the name, the root hash, the state, the block progress, and the time.
The create form takes a root hash and a file name, which is the same pair the share text carries.
The detail shows the progress and, after `Completed`, a download link that points at the content route, so the browser's own download manager handles the transfer and its resume.

### 3.4 Creating a publication

`/publications/new` offers two sources.

- **Upload.** The browser reads the chosen file, sends it in chunks (§4.2), and shows the stored progress.
- **Library.** The client lists the configured roots and one directory at a time, and sends the root identifier and the relative path of the chosen file.
  The daemon starts the import immediately, and the client polls the publication.

The client does not let the user type a path.
The library browser is a read-only view of the configured roots; an entry outside the root is not offered.

### 3.5 Navigation and shared state

The header shows a link to the publications and subscriptions and a logout action.
Server state lives in the query cache; the client keeps no global store.
A mutation invalidates the list it changes, so a screen never shows a state the daemon has already left.

## 4. Data flow

### 4.1 Polling

A resource in a non-terminal state is polled every 1 second.
A terminal state stops the poll.
A list is polled every 5 seconds while the tab is visible and is refetched when the tab becomes visible or the window regains focus.

The client polls the resource it shows rather than a global event stream, so closing a tab stops the load it causes and reopening it reconstructs the state from the row.

### 4.2 Resumable upload

The client slices the file into 8 MiB chunks and sends them in order with `Content-Range: bytes <start>-<end>/<size>`.
A zero-byte file sends no content request; the publication is created already in `Pending` and the client only polls it.
It reads the accepted offset from the resource and never assumes that a sent chunk was stored.

| Response | Client action |
| --- | --- |
| `2xx` | Send the next chunk |
| `409` with `receivedBytes` | Continue from `receivedBytes` |
| `413` | Stop and show the declared size and the configured limit |
| `507` | Stop and show that the destination is full |
| Network error or `5xx` | Retry with a backoff from 1 second to 30 seconds, then continue from `receivedBytes` |

Progress is the ratio of `receivedBytes` to `size`, not the number of bytes the client has sent, so a retry shows the stored progress.
A page unload while the publication is `Receiving` warns the user.
The publication remains after the page closes, and the next visit resumes from `receivedBytes`.

### 4.3 Errors and recovery

| Status | Client action |
| --- | --- |
| `401` | Move to `/login` and keep the requested route |
| `403` | Show that the request came from another origin and that the cookie policy is the cause |
| `404` | Show that the resource is gone and return to the list |
| `5xx` | Keep the screen and retry with a backoff |

The client never hides a failed transfer.
The reason the daemon recorded is shown in the detail, and the list keeps the terminal row until the operator deletes it.

## 5. Build and development

### 5.1 Stack and layout

The client lives in `ui-web/` in this repository, next to `daemon/`, so that the API contract and the client change in one review.

| Layer | Choice |
| --- | --- |
| Build | Vite |
| UI | React 19, TypeScript, Tailwind CSS, shadcn/ui on Radix |
| Routing | TanStack Router |
| Server state | TanStack Query |
| API types | `openapi-typescript` generated from `daemon/openapi.yaml` and committed, `openapi-fetch` as the client |
| Lint and format | Biome |
| Tests | Vitest for units, Playwright for the browser flow |

The generated type file is not edited by hand.
Regenerating it from `openapi.yaml` is the only way it changes, and the result is committed, for the same reason as the Rust interface ([daemon-api.md §4.2](./daemon-api.md#42-code-generation-from-openapi)).

User-visible strings live in one module.
The first version ships one language, and a second language does not touch the components.

### 5.2 Development proxy and production build

In development, Vite serves the client on its own port and proxies `/api` to `api.listen_addr`, so the browser sees one origin and the cookie behaves as it does in production.
No CORS setting exists on either side.

The production build is a static directory that the operator places where `web.dir` points, and the daemon serves it ([daemon-api.md §7.1](./daemon-api.md#71-same-origin-static-serving)).
The build is reproducible from the checked-in sources.
The generated API types are committed, and the CI checks that regenerating them changes nothing, so the build does not need the generator.

## 6. Design decisions

### 6.1 Decided

#### Serve a single-page application without server-side rendering

**Decision**
The client is a static single-page application built by Vite and served by the daemon.

**Rationale**
The client is a private operational screen with no search engine and no first-paint requirement beyond a local network.
A static build keeps the daemon free of a template engine and a Node runtime.

**Rejected**
Server-side rendering adds a runtime to the daemon's process or a second process to deploy, for a screen that no stranger sees.

#### Keep the session in an `HttpOnly` cookie

**Decision**
The browser uses the cookie transport, and no script reads or writes the session token.

**Rationale**
The page renders names and failure reasons that come from remote peers, so any script injection would also be able to read a token kept in web storage.
The cookie is also what makes the same-origin rule in [daemon-api.md §6.3](./daemon-api.md#63-cross-origin-requests) meaningful.

**Rejected**
A bearer token in `localStorage` survives a browser restart without a server-side row, but it widens the effect of any script injection and needs a refresh rule.

#### Poll the resource the screen shows

**Decision**
The client polls each non-terminal resource every second and stops at a terminal state; it does not open a stream.

**Rationale**
The resource is durable, so a reload reconstructs the screen with no resume rule, and the poll load follows what the operator is looking at.

**Rejected**
A shared event stream would carry every resource to every open tab, including the ones the operator is not looking at, and needs a reconnect and replay contract.

#### Upload from the browser in chunks

**Decision**
The client slices the file and sends ranges, and it resumes from the offset the daemon reports.

**Rationale**
A LAN or VPN connection drops, and a multi-gigabyte upload that restarts from zero is not usable.
The daemon's offset is the only progress the client trusts.

**Rejected**
One request with the whole file is simpler but cannot resume, and the daemon would need a second path for large files.

#### Download by navigation, not by fetching into memory

**Decision**
The download link points at the content route, and the browser handles the transfer.

**Rationale**
The browser's download manager already implements resume, disk placement, and a progress UI, and the page never holds the file in memory.

**Rejected**
Fetching the content into a `Blob` and saving it lets the client show its own progress bar, but it doubles the memory and loses the browser's resume.

#### Generate the API client types from OpenAPI

**Decision**
The client's request and response types are generated from `daemon/openapi.yaml`, committed as `ui-web/src/api/schema.d.ts`, and checked in the CI by regenerating them.
The build does not regenerate them.

**Rationale**
The same contract produces the Rust router and the client's types, so a field that changes on one side fails the other side's type check.
Committing the result makes a contract change visible in review and keeps the build independent of the generator, as the Rust interface already does.

**Rejected**
Hand-written types drift silently and turn a contract change into a runtime error.
Generating at build time keeps the tree smaller, but it hides the contract change from review and makes every build depend on the generator.

### 6.2 Deferred

#### A second language

**Current state**
The client ships one language.

**Why not decided yet**
The operator is the only user, and no screen has been requested in another language.

**Decision condition**
Decide when a user who does not read the current language operates the node.

#### A PWA and offline screens

**Current state**
The client has no service worker and no offline cache.

**Why not decided yet**
Plain HTTP on the LAN does not provide the secure context a service worker needs.

**Decision condition**
Decide when a deployment serves HTTPS and the operator asks for an installed client.

#### Notifications when a transfer finishes

**Current state**
The client shows the terminal state only while a screen is open.

**Why not decided yet**
A desktop notification needs permission and a secure context, and the first client does not stay open for the whole of a long transfer.

**Decision condition**
Decide when a transfer that outlives the tab becomes common, or when a deployment serves HTTPS.

#### Publishing several files at once

**Current state**
One publication is created per file, and the client sends them one at a time.

**Why not decided yet**
The API has no batch creation, and a queue in the client would hide partial failures.

**Decision condition**
Decide when an operator publishes a directory rather than a file, and the per-file contract proves too slow.

## 7. Current status and remaining work

`ui-web/` implements the setup and login screens, the publication list, detail, upload, and library picker, and the subscription list, detail, and download, on TanStack Router and Query.
The daemon serves the build from `web.dir` ([daemon-api.md §10](./daemon-api.md#10-current-status-and-remaining-work)).
Vitest covers the upload, the polling rules, and the redirect guard, and a Playwright flow starts two daemons and drives setup, login, upload, library publication, subscription, download, and deletion through the build.
The setup procedure is in the [README](../../README.md).

For confirmed bugs, see [issues.md](../issues.md).
