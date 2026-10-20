# Axus Known Issues

This document covers clear bugs confirmed by reading the code.
Deferred design decisions live in the "Design decisions" sections of the corresponding design documents.

## 1. About this document

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | The list of code-confirmed bugs, the relations between items, and tracking until fixed |
| [design.md](./design.md#11-division-of-responsibilities-among-documents) | Design decisions and implementation status |
| [terms.md](./terms.md#11-division-of-responsibilities-among-documents) | Term definitions and notation |

The map of the document family is held by [design.md](./design.md#11-division-of-responsibilities-among-documents).

### 1.2 Usage

- When a GitHub Issue is filed, append the number to the Issue column of the list.
- When fixed, remove the list row and the child document.
- Line numbers drift with code changes, so re-check the relevant locations when fixing.

## 2. List

| Item | Severity | Investigated on | Issue |
| --- | --- | --- | --- |
| [Lock file of an abnormally terminated daemon remains and prevents restart](./issues/daemon-lock-survives-abnormal-exit.md) | Medium | 2026-10-06 | - |
| [No deadline for send_message and an accept-waiting Session after establishment](./issues/session-accept-and-message-without-deadline.md) | Medium | 2026-10-07 | - |
| [output_lock is common across all subscriptions and a writing subscription stops other subscriptions' finalization](./issues/output-lock-shared-across-subscriptions.md) | Medium | 2026-10-07 | - |
| [A vanished destination directory of Failed or Canceled rows makes sweep retries continue and remove impossible](./issues/vanished-output-directory-blocks-sweep-and-remove.md) | Medium | 2026-10-07 | - |
| [delete_bulk succeeds deleting nothing when it contains a missing key](./issues/delete-bulk-deletes-nothing-on-missing-key.md) | Low | 2026-10-06 | - |
| [TaskCommunicator::start does not exit its loop even when the accept channel is closed](./issues/task-communicator-spins-on-closed-session-channel.md) | Low | 2026-10-07 | - |
| [SessionConnector accepts the Unknown use-selection result](./issues/session-connector-accepts-unknown-result.md) | Low | 2026-10-11 | - |
| [The dialing side does not reconcile a candidate's public key with the Session cert](./issues/dialer-does-not-match-candidate-public-key.md) | Medium | 2026-10-12 | - |

## 3. Not included here

| Item | Reference |
| --- | --- |
| No FileExchanger block exchange protocol | [file-transfer.md](./design/file-transfer.md#7-current-status-and-remaining-work) |
| No REST API to start publishing and subscribing | [file-transfer.md](./design/file-transfer.md#7-current-status-and-remaining-work) |
| No NodeFinder active lookup | [node-finder.md](./design/node-finder.md#6-design-decisions) |
| REST API reach and authentication undecided | [daemon-api.md](./design/daemon-api.md#reach-and-authentication-of-the-rest-api) |
| Web of Trust not designed | [trust-security.md](./design/trust-security.md#4-design-decisions) |
| Profile and memo not implemented | [profile-memo.md](./design/profile-memo.md#5-current-status-and-remaining-work) |
| Long-running REST operation representation undecided | [daemon-api.md](./design/daemon-api.md#5-design-decisions) |
