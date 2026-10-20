# Axus External Knowledge

This document covers facts about depends and the OS confirmed against primary sources.
Axus design decisions and implementation status live in the design documents, and code bugs in the issues.

## 1. About this document

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | The list of external facts, the verification targets, the sources, the confirmation dates, and the relations between items |
| [design.md](./design.md#11-division-of-responsibilities-among-documents) | The map of the document family and design decisions that assume external specifications |
| [terms.md](./terms.md#2-terminology) | Definitions and notation of project terms |
| [issues.md](./issues.md) | Implementation bugs found when checked against external specifications |

### 1.2 Usage

When a dependency version, supported OS, or external specification changes, reconcile the verification targets and update the facts and evidence.
Do not keep descriptions of past versions in the body.
For "literature only", re-read the primary source every 3 months from the reference date, and advance the confirmation date if the content is unchanged.
For "literature and runtime verification", the entry includes facts reproduced in a real environment in addition to the primary source, and does not include fault injection tests.
Items without fault injection tests are not handled as "verified".

## 2. List

| Item | Verification target | Source | Last confirmed |
| --- | --- | --- | --- |
| [RocksDB power-loss durability depends on the sync flag and build-time definitions](./knowledges/rocksdb-write-durability.md) | rocksdb 0.24.0 / librocksdb-sys 0.17.3+10.4.2 / cc 1.2.56 | Verified | 2026-10-06 |
| [SQLite WAL power-loss durability depends on the sync conditions](./knowledges/sqlite-wal-durability.md) | SQLx 0.9.0 / libsqlite3-sys 0.30.1 / bundled SQLite 3.46.0 | Literature only | 2026-10-04 |
| [File I/O completion alone does not align content and directory entry persistence](./knowledges/filesystem-sync-durability.md) | Tokio 1.52.3 / Rust std 1.96.0 / Linux man-pages 6.19 / Apple fsync(2) archived manual | Literature and runtime verification | 2026-10-07 |
| [A non-replacing rename is provided by a different API on each OS](./knowledges/filesystem-output-publication.md) | Linux man-pages 6.19 / macOS 26.6.2 rename(2) / Microsoft Learn MoveFileExW / Rust std 1.96.0 | Literature and runtime verification | 2026-10-07 |

The RocksDB and SQLite syncing are the premise of the order that finalizes metadata after the bodies.
The filesystem syncing and non-replacing rename additionally relate to finalizing a subscription output and judging placement at restart.
Because Windows directory entry persistence depends on the rename item's `MOVEFILE_WRITE_THROUGH`, the two filesystem items are re-read together.
The dependent sections of [storage.md](./design/storage.md#1-about-this-document) are referenced from each item's implications.
