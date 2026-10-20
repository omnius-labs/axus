# Design of Profile and memo

## 1. About this document

This document covers the responsibility boundary through which higher-level services reference public data from a Profile and through which a memo distributes that Profile.
Term definitions are the source of [terms.md](../terms.md).

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | Profile references, memo distribution, and the boundary with higher-level services |
| [file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries) | Publishing files, lookup, block transfer |
| [trust-security.md](./trust-security.md#4-design-decisions) | The subject behind the Profile and trust judgments |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §5.

## 2. Responsibilities and boundaries

A Profile does not embed post bodies or attachment byte strings directly; it holds FileRefs to published files.
Both a file grouping a set of posts and files individually published by a user are first handed to FilePublisher to obtain a root hash.
The higher-level service layer builds FileRefs from that result and registers them following the Profile's classification rules.

This structure avoids stacking the distribution of large data and the update of a Profile onto the same protocol.
Even when a Profile's authenticity is verified, the block hash verification of the referenced files is not omitted.

## 3. Responsibilities by layer

| Layer | Responsibility | Does not know |
| --- | --- | --- |
| FilePublisher and FileExchanger | Publishing files, lookup, block transfer | The meaning of posts and Profiles |
| MemoExchanger | Discovery, exchange, and distribution of Profiles | Interpretation of post bodies and attachments |
| Higher-level service layer | Building post data, classifying FileRefs, creating and updating Profiles | Details of block transfer |

The lower layers treat a Profile as an opaque payload, and only the higher layer interprets the meaning of FileRefs.
This boundary means changing the data model of a bulletin board or timeline does not require rebuilding memo's discovery and distribution.

## 4. Design decisions

### 4.1 Decided

#### Profiles reference public data through FileRefs

**Decision**
A Profile does not embed post bodies and attachments; it holds FileRefs to data published with FilePublisher.
MemoExchanger does not interpret a Profile's meaning and only discovers, exchanges, and distributes it.

**Rationale**
To consolidate content distribution into the existing file protocol and to separate the Profile and bulletin-board models from the transport.

### 4.2 Deferred

#### Format and update of Profiles

**Current state**
Only the responsibility boundary is defined: a Profile holds a set of FileRefs, and a memo distributes the Profile.

The candidates are the following two.

1. Chaining versioned immutable Profiles makes history verification easy, but requires finding the latest version and reclaiming storage.
2. Distributing a mutable latest version per subject makes fetching simple, but requires rules for conflicts, rollback, and replay.

**Why not decided yet**
Because the authenticity and conflict resolution of a Profile cannot be defined until the signed targets of identity and Web of Trust are decided.

**Decision condition**
Decide before implementing any of the Profile type, MemoRef, or MemoExchanger as a protocol surface.

## 5. Current status and remaining work

The FileRef type exists, but the memo and MemoRef modules are empty, and the Profile type and the higher-level service layer do not exist.
After the identity and trust judgments, define the Profile's format, update rules, and memo's protocol surface.
