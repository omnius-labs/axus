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

- When a bug is confirmed, add a child document and a row to the list.
- When fixed, remove the list row and the child document.
- Line numbers drift with code changes, so re-check the relevant locations when fixing.

Confirmed bugs are tracked in this document family and are not filed as GitHub Issues.

## 2. List

| Item | Severity | Investigated |
| --- | --- | --- |
| [TaskCommunicator::start does not exit its loop even when the accept channel is closed](./issues/task-communicator-spins-on-closed-session-channel.md) | Low | 2026-11-12 |

No relation to another item is known.

## 3. Not included here

| Item | Reference |
| --- | --- |
| File search is not designed | [web-of-trust.md §5.3](./design/web-of-trust.md#53-subscription-and-search) |
