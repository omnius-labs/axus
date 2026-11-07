# Plan: the browser client

## 1. About this plan

This plan builds the browser client of [web-client.md](../design/web-client.md#1-about-this-document) on top of the implemented REST API.
It covers the setup, login, publication, and subscription screens and the build that the daemon serves from `web.dir`.

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | The order, steps, and acceptance checks of this work |
| [web-client.md](../design/web-client.md#1-about-this-document) | Screens, routes, polling, upload, and build |
| [daemon-api.md](../design/daemon-api.md#5-rest-resources) | REST resources and authentication |
| [openapi.yaml](../../daemon/openapi.yaml) | Paths, schemas, status codes |

### 1.2 Creation date and target commit

Created on 2026-11-07 against `6ee6058`.

## 2. Purpose and completion conditions

An operator can set up the owner, log in, publish an uploaded or library file, watch it commit, subscribe to a root hash, and download the output from a browser, with the daemon serving the build from `web.dir`.

### 2.1 Completion conditions

1. `npm run build` produces a static build from the checked-in sources.
2. With `web.dir` pointing at the build, the daemon serves it at `/`, and an unknown client route returns `index.html`.
3. The setup and login screens create the owner and a session, and a wrong password shows an error.
4. The publication list shows committed and active publications, and the detail polls until a terminal state.
5. An upload sends 8 MiB chunks with `Content-Range`, resumes from `receivedBytes` after a `409`, and shows stored progress.
6. Selecting a library file lists the roots and one directory at a time and publishes the chosen file.
7. The subscription list starts a subscription by root hash and name, polls to `Completed`, and downloads the output.

### 2.2 Non-goals

- A second language, a PWA, and notifications (deferred in the design).
- A node information screen and share links.
- Server-side rendering.

## 3. Current state

The daemon serves the API under `/api/v1` and a static directory from `web.dir`, with an `index.html` fallback.
`ui-web/` holds the screens, the browser flow test, and the README procedure.

## 4. Approach

The client is a Vite single-page application in `ui-web/`, served by the daemon from the same origin.
It uses React 19, TanStack Router and Query, Tailwind CSS, the generated OpenAPI types, and Biome.
The development server proxies `/api` to `api.listen_addr`, so the cookie behaves as in production.

### 4.1 Decisions this plan depends on

| Decision | Source |
| --- | --- |
| Same-origin SPA served from `web.dir` | [daemon-api.md §9.1](../design/daemon-api.md#91-decided) |
| Cookie session with `SameSite=Strict` and the origin check | [daemon-api.md §6.3](../design/daemon-api.md#63-cross-origin-requests) |
| 8 MiB chunked upload with resume from `receivedBytes` | [web-client.md §4.2](../design/web-client.md#42-resumable-upload) |
| Download by navigation | [web-client.md §6.1](../design/web-client.md#61-decided) |

### 4.2 Impact

- Adds `ui-web/`; no Rust change is required for the build itself.
- The generated API types are part of the build and are not committed.

## 5. Steps

| # | Step | Depends on | State |
| --- | --- | --- | --- |
| 1 | Create the Vite and TypeScript scaffold with the generated API client | - | Complete |
| 2 | Add the setup and login screens | 1 | Complete |
| 3 | Add the publication list, detail, and upload | 2 | Complete |
| 4 | Add the library picker | 3 | Complete |
| 5 | Add the subscription list, detail, and download | 2 | Complete |
| 6 | Add the browser tests and the README procedure | 3, 5 | Complete |

<a id="s-1"></a>
### Step 1. Create the Vite and TypeScript scaffold

**Changed locations**
`ui-web/package.json`, `ui-web/vite.config.ts`, `ui-web/tsconfig.json`, `ui-web/src/`.

**What to do**
Add Vite, React 19, TanStack Router and Query, Tailwind, the generated OpenAPI types, and Biome. The dev server proxies `/api` to `127.0.0.1:5050`.

**Completion condition**
`npm run build` produces `ui-web/dist/index.html` and the script assets.

## 6. Acceptance verification

| Condition | Command | Expected result |
| --- | --- | --- |
| 1 | `cd ui-web && npm ci && npm run build` | `dist/index.html` exists |
| 2 | a daemon started with `web.dir = ui-web/dist` and `GET /unknown` | Returns `index.html` |
| 3-7 | `cd ui-web && npm run test:e2e` | The browser flow passes against two daemons |

## 7. Risks and rollback

- The generated API types come from `daemon/openapi.yaml`; a contract change without regenerating breaks the build. The `prebuild` script regenerates them.
- The build is static; reverting the directory removes the client without touching the daemon.
