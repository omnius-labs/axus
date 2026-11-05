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

There are no known issues.

## 3. Not included here

| Item | Reference |
| --- | --- |
| No NodeFinder active lookup | [node-finder.md](./design/node-finder.md#6-design-decisions) |
| Web of Trust not designed | [trust-security.md](./design/trust-security.md#4-design-decisions) |
| Profile and memo not implemented | [profile-memo.md](./design/profile-memo.md#5-current-status-and-remaining-work) |
