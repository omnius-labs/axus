# Design of persistence

## 1. About this document

This document covers the design of node and file metadata, block bodies, subscribed output files, the state directory, and schema migration.
Term definitions are the source of [terms.md](../terms.md#2-terminology).

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | Persistence of metadata, blocks, and output files, finalization and reclamation procedures, startup recovery, the state directory, migration |
| [file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries) | File state transitions, output reservation, block verification, Store and Task responsibilities |
| [knowledges.md](../knowledges.md) | Evidence on the external specifications of RocksDB, SQLite, and filesystem syncing and placement |
| [base/storage](../../daemon/modules/engine/src/base/storage) | Block storage implementation |
| [core/negotiator](../../daemon/modules/engine/src/core/negotiator) | SQLite repository and migration implementation |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §6.
To check what works currently, read §6 first.

## 2. Responsibilities and boundaries

The persistence layer handles metadata, which needs state transitions and conditional queries, and block bodies, which are read and written by hash key, with separate storage models.
Subscription outputs are placed on the filesystem, and the temporary output and destination are derived from the subscription row to tie the DB state to the placement.
It does not own the file protocol, lookup rules, or the REST API contract.

## 3. Separation of metadata and blocks

| Kind | Body | Persists |
| --- | --- | --- |
| metadata | SQLite and `omnius-core-migration` | Known nodes, file state and destination, block index |
| block | `KeyValueRocksdbStorage` | Byte strings of file bodies and MerkleLayers |
| Subscription output | Filesystem chosen by the user | Temporary output and finalized output files |

Metadata is placed in SQLite because it needs conditional queries and state transitions.
Blocks are placed in RocksDB because reads and writes by hash key are central and the large byte strings are separated from metadata queries.

### 3.1 Structure of `state_dir`

Persistent state is split into a directory per component.

```text
<state_dir>/
├── identity/
├── repo/
├── file_publisher/
│   ├── repo/
│   └── blocks/
└── file_subscriber/
    ├── repo/
    └── blocks/
```

Separating the publisher and subscriber storage areas prevents rebuilding or deleting one from directly breaking the other's state.
Because a temporary output is placed at the destination by rename, it is created in the destination's parent directory and not in `state_dir`.
`identity/` holds the node's signing key; deleting it makes the next startup create a different key and node ID, so unlike other directories it cannot be undone by recreation.
Adding or migrating a directory name requires declaring a unit of migration and rollback.

## 4. Logical keys and migration

### 4.1 Logical keys

[KeyValueRocksdbStorage](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs) separates `names`, which resolves a logical key to an internal ID, from `blocks` and `metas`, which resolve an internal ID to a body.
A publish commit changes the logical key of a task from uncommitted to committed, reusing the same block bodies.

The renames of multiple blocks in one commit are combined into a single RocksDB transaction, so a state where only some blocks moved does not remain.
This atomicity assumes the targets are in the same RocksDB database and does not hold for moving to another database.
There is no atomicity combining the rename and the SQLite commit, and a failure between them is reclaimed by the §4.3 sweep.

### 4.2 Durability guarantees and boundaries

After a successful save or finalize, the metadata and bodies that indicate it are retained across process exit, OS crash, and power loss.
The targets are Linux, macOS, and Windows, and the guarantee assumes the OS, filesystem, and device honor sync requests and write order.
Device failure and a user's modification or deletion of a finalized output are not included in this save guarantee.
A sync failure is handled as an operation failure, and a success that weakens the guarantee is not returned.

Returning a successful save or finalize requires the following syncs to have succeeded in addition to the completion of asynchronous I/O or `flush`.

| Destination | Condition that settles success |
| --- | --- |
| RocksDB | Enable WAL and persist block additions, logical key changes, and deletions with `sync=true` writes. On macOS, build RocksDB with `HAVE_FULLFSYNC` defined so it uses `F_FULLFSYNC` for syncing |
| SQLite | Set WAL and `synchronous=FULL` explicitly on each connection. On macOS, also set `fullfsync=ON` |
| Output file | After the decode writer's `flush`, persist the contents with `sync_all` |
| Destination directory entry | On Linux and macOS, `sync_all` the parent directory after placement. On Windows, specify `MOVEFILE_WRITE_THROUGH` for placement |

The bodies are persisted first, and the metadata indicating them is finalized afterwards.
If this order breaks, finalized metadata can reference a body lost to a power failure.
The entries for creating and deleting a temporary output are not synced. If a created entry is lost, the §4.5 recovery turns it into Failed.
If a deleted entry returns after a power failure and the subscription row no longer exists, the temporary output remains in the destination directory. The sweep derives temporary output names from rows, so it does not reclaim this file.
For external specifications, see [RocksDB syncing](../knowledges/rocksdb-write-durability.md), [SQLite syncing](../knowledges/sqlite-wal-durability.md), and [file and directory syncing](../knowledges/filesystem-sync-durability.md).

### 4.3 Publishing commit and reclamation

The Store performs the following inside the publisher mutex. Encoding happens outside it.

1. Check for a Processing row and whether a committed file with the same root hash exists.
2. If no committed file exists, delete the committed keys remaining under that root hash and move the logical keys of the blocks, minus duplicate hashes, from uncommitted to committed. The deletion and all renames are combined into one RocksDB transaction and synced. If a committed file exists, share its bodies and do not change them.
3. Register the committed file and the block index in one SQLite transaction and delete the uncommitted file row. After this transaction is persisted, commit success is returned.

A failure between 2 and 3 can leave committed keys unreferenced by any committed file. The next commit with the same root hash deletes these keys in step 2 before moving, so there is no collision.
These keys and the uncommitted keys of temporary IDs that have no row are reclaimed by the sweep.
Reclamation targets are not recorded in advance. Whether deletion is safe is judged only by the SQLite references at deletion time.

The sweep runs inside the mutex, reconciles RocksDB keys against SQLite rows, and deletes and syncs unreferenced keys.

| Key | Condition for deletion |
| --- | --- |
| uncommitted | No Pending or Processing row for that temporary ID |
| committed | No committed file for that root hash |

When the result of an SQLite commit is unknown, the sweep re-reads SQLite at deletion time, so it does not delete the blocks of a committed file that was finalized.
If SQLite cannot be read, the sweep fails without deleting anything.

The sweep runs at Store open, and while running, after a commit failure and after a body-deletion failure during cancel/remove.
TaskEncoder retries it with increasing intervals until the sweep succeeds. Whether a sweep is needed is held only in memory, and on restart the open sweep takes over.
Because the sweep scans all keys, its cost scales with the number of stored blocks.

### 4.4 Subscribed block storage and reference deletion

A received block is written to RocksDB and persisted before the SQLite downloaded and progress are persisted.
When batches of syncs are combined, none of the downloaded entries is finalized before the whole batch is persisted.
If only the body remains, it can be re-received or re-registered, but a state where downloaded has no body is not treated as a normal recovery result.

remove deletes the last reference from SQLite and then deletes the root body inside the Store's write lock.
A root whose body deletion failed is reclaimed by the sweep as a root unreferenced by subscriptions.
The subscriber sweep re-reads the subscription rows inside the write lock and does not delete the bodies of referenced roots.
The publisher remove also follows the same order, using the publisher mutex for the reference check.

### 4.5 Output finalization and recovery

The destination is recorded on the subscription row as a pair of the resolved absolute path of the parent directory and the final entry name.
A partial unique index is placed over the Downloading, Decoding, and Finalizing rows to reject a new subscription to the same destination while one is in progress.
When the state changes to Completed, Failed, or Canceled, or the row disappears, the row leaves the unique index scope in the same transaction.
The destination unique constraint compares names as written, so an alternative representation differing only in letter case or Unicode normalization is not rejected at registration and is detected at finalization-time placement.
The existing entry check does not follow symlinks, and a dangling symlink is also treated as a conflict.

**The temporary output name is `.<entry name>.axus-<subscription ID>.part` in the same directory as the destination, and the daemon owns files with this name.**
Ownership is judged by this name alone, and no file identity is recorded.
Names of this shape are reserved and cannot be registered as the final destination of a user's file.
The registration-time judgment uses only the name string, and the subscription ID format follows temporary output generation.
Name generation and reserved-name judgment live in the same type's `impl`.
Accepting subscription A's temporary output name as subscription B's final destination would make A, when it recreates its temporary output after B finishes, delete B's completed output; this is why the name is reserved.
Only for the reserved-name judgment, ASCII letter case is not distinguished at registration as an exception.
`.axus-`, `.part`, and the hex part of the subscription ID match in either case.
The deletion when recreating a temporary output happens before the rename, so detecting a reserved name differing in letter case at finalization-time placement cannot protect a completed output.
Alternative representations of other names differing only in letter case or Unicode normalization are not handled at registration and are detected at finalization-time placement.
Unicode uppercase/lowercase correspondences are not included in the reserved-name judgment.
On filesystems that equate a non-ASCII character with a reserved-name character, such as identifying `ſ` (U+017F) with `s`, the same deletion problem can remain.
This convention assumes external processes do not create, delete, or rename files with this name.
If the premise breaks, restart may misjudge whether placement happened and mark a placed subscription Failed, or treat an external file as a temporary output.

The finalization order is fixed as follows.

1. The Store deletes an existing file at the temporary output name, creates a new one, and hands a writer to the Task. If it cannot be created, such as when the name is too long, it becomes Failed. The Task decodes only into the temporary output.
2. After the writer's `flush`, the Store `sync_all`s the temporary output.
3. In one SQLite transaction, conditionally update Decoding to Finalizing. If cancel/remove is already finalized, do not place and delete the temporary output. This is where finalization begins.
4. Place the temporary output at the destination with a rename that does not replace an existing entry, and sync the directory entry per §4.2.
5. Persist Completed in SQLite. Success is returned after this.

The non-replacing rename uses `renameat2`'s `RENAME_NOREPLACE` on Linux, `renamex_np`'s `RENAME_EXCL` on macOS, and `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` on Windows.
`std::fs::rename` is not used because it replaces an existing destination.
When the rename fails because an entry exists at the destination, or the store does not support this rename, the subscription becomes Failed and the temporary output is deleted.
When the rename fails because there is no temporary output, it is handled the same as "neither temporary output nor destination" at restart: Completed if the destination exists, otherwise Failed.
For other I/O failures, Finalizing is kept, and TaskDecoder retries placement in the Store with increasing intervals.
For external specifications, see [non-replacing rename](../knowledges/filesystem-output-publication.md).

A rename in one operation removes the temporary output name and creates the destination name, so at restart the presence of the two names tells whether placement finished.

| State at restart | Handling |
| --- | --- |
| Temporary output exists, destination does not | Exited before placement. Retry from step 4 |
| Temporary output exists, destination also exists | An external process created the destination after the start. Fail and delete the temporary output, keep the destination |
| No temporary output, destination exists | Exited after placement. Sync the parent directory, then record Completed |
| Neither temporary output nor destination | The temporary output was lost. Mark Failed. The blocks remain, so subscribing again decodes it again |
| Cannot reach the parent directory, or cannot judge due to an I/O failure | Keep Finalizing, and let TaskDecoder retry the judgment while running |

The last row's example is restarting with the destination disk removed.
Only this subscription remains Finalizing, and it does not stop other subscriptions or the Store open.

Cancel before finalization first performs a conditional update from Downloading or Decoding to Canceled, then waits for the Task's writer to stop and deletes the temporary output.
Because the update is finalized first, the Task does not advance to Finalizing and does not try to place a deleted temporary output.
Even if the temporary output deletion fails, Canceled is not returned, and the leftover temporary output is deleted by the sweep.
Remove before finalization runs the same conditional update to Canceled, deletes the temporary output, and then deletes the row. If the deletion fails, it leaves the Canceled row and fails the remove. As long as the row remains, the temporary output's name can be derived from the row.
The subscriber sweep deletes, in addition to the bodies of unreferenced roots, the temporary outputs pointed to by Canceled and Failed rows.

### 4.6 Startup recovery

The Store accepts the Task and the API only after recovery.
If the SQLite or RocksDB of `state_dir` cannot be read or written, the open fails and new operations are not accepted in an ambiguous state.
An unjudgeable condition caused by the destination directory is deferred per subscription per §4.5 and does not fail the open.

| Store | Startup recovery |
| --- | --- |
| FilePublisherStore | Delete all uncommitted keys and return Processing to Pending, then run the sweep. Because encoding is not running inside open, the keys of Pending rows may also be deleted |
| FileSubscriberStore | Recover Finalizing per the §4.5 rules. Resume Downloading and Decoding from the stored blocks, and for rank 0 decoding recreate the temporary output in §4.5 step 1. Then run the sweep |

Every operation can be re-executed any number of times from persisted information.
Both the processing order and the syncing prevent finalized metadata from referencing an unpersisted body.

### 4.7 Migration

[repo.rs (publisher)](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs), [repo.rs (subscriber)](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/repo.rs), and [node_finder_repo.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs) apply named MigrationRequest objects in order.
A migration whose name was already applied is not run again.

A migration that may be applied to a distributed state is not rewritten; progress is made with a new-named migration.
Even when directly fixing a pre-first-release migration, confirm that no applied state exists and record that confirmation in the change record.
For the addition of Finalizing and the destination unique constraint, the initial inactive schema was changed, and no compatibility handling for migrating old state is provided.

## 5. Design decisions

### 5.1 Decided

#### Place metadata and block bodies in separate storage

**Decision**
Metadata requiring search and state transitions is placed in SQLite, and block bodies handled by hash key in RocksDB.

**Rationale**
To avoid forcing relational queries and large byte-string reads and writes into the same storage model, and to match each access pattern.

#### Include OS crashes and power loss in the save guarantee

**Decision**
Persisting a body is completed first, and the metadata indicating it is persisted afterwards.
For syncing, only each storage's sync setting and std's `sync_all` and the per-OS non-replacing rename are used.
An output file syncs the placed directory entry before recording Completed.

**Rationale**
To not make normal process exit the condition for saving, and to retain returned files and progress after a failure.
Write order alone cannot protect bodies lingering in the OS buffer and device cache.
Limiting the sync means to standard calls avoids a per-destination capability check and only needs to return a sync failure as an operation failure.

**Rejected**
Guaranteeing only process exit reduces synchronous writes, but a power loss after recording completion can lose the body, so it is not taken.
Judging in advance whether a destination supports the required syncing and rejecting unsupported destinations requires per-OS and per-filesystem judgment, so it is not taken.

#### Own temporary outputs by name and finalize with a non-replacing rename

**Decision**
A temporary output is created with a conventional name including the subscription ID, and daemon ownership is judged by that name alone.
Finalization uses a rename that does not replace an existing entry, and at restart the presence of the temporary output and destination tells whether placement happened.

**Rationale**
A rename swaps the two names in one operation, so whether placement happened can be judged without recording file identity.
Because the temporary output name can be derived from the subscription row, no separate record is needed for reclamation.

**Rejected**
Recording a temporary output's identity and adding a hard link at the destination, keeping the temporary-side link until Completed, requires per-OS file identity for the ownership check and must handle files of unknown ownership when the process exits before recording the identity.
A design that checks existence and then places with `std::fs::rename` replaces an external file created between the check and the placement.

#### Reclaim unreferenced bodies by reconciliation

**Decision**
The keys of unreferenced blocks and temporary outputs are reclaimed by a sweep that reconciles against SQLite references at deletion time.
Reclamation targets are not recorded in advance or after a failure.

**Rationale**
Judging deletability by current references alone avoids writing reclamation information per kind of failed operation.
Because the renames of one commit are combined into a single RocksDB transaction, without recording targets in advance the only leftovers are unreferenced keys.

**Rejected**
Recording the commit targets before moving blocks and persisting the failures for retry requires a record write order, a record table, and refusing re-import until reclamation finishes.

### 5.2 Deferred

There are no deferred decisions closed to this concern.

## 6. Current status and remaining work

The NodeFinder, publisher, and subscriber repositories and the block storage exist.
In the Store open, the publisher and subscriber first finish the block reclamation described below, then accept the Task and entry operations.
The publisher deletes all uncommitted keys, returns Processing to Pending, and then reclaims unreferenced keys with the sweep.
The subscriber recovers Finalizing outputs per the §4.5 rules and deletes the keys of root hashes unreferenced by subscriptions and the temporary outputs of Canceled and Failed.
Subscriptions that cannot be judged because the destination is unreachable remain Finalizing, and the recovery and open of other subscriptions proceed.

All RocksDB writes are `sync=true` with WAL enabled.
Each SQLite connection of NodeFinder, the publisher, and the subscriber sets WAL and `synchronous=FULL`, and `fullfsync=ON` on macOS.
Subscribed blocks are persisted in RocksDB and then the SQLite downloaded and progress are persisted.

A publish commit syncs the deletion of orphaned keys under the same root and the rename of all blocks minus duplicate hashes in one RocksDB transaction, then finalizes the metadata in one SQLite transaction.
When a committed file with the same root exists, the bodies and block index are shared.
The publisher's runtime sweep reads the SQLite references inside the mutex at deletion time and deletes and syncs unreferenced keys.
The sweep runs after a commit or body-deletion failure, and while it keeps failing, TaskEncoder retries with increasing intervals.
The need for a retry is held only in memory.

The destination is stored as the resolved parent directory's absolute path and the final entry name, and a partial unique index over in-progress subscriptions and a no-symlink-check on existing entries reject conflicts at registration.
Reserved temporary output names are rejected at registration without distinguishing ASCII letter case.
Rank 0 is decoded into the temporary output, and after the writer's `flush` and the file's `sync_all`, Finalizing is recorded with a conditional update.
Placement uses the per-OS non-replacing rename, and Completed is recorded after the parent directory's `sync_all` on Linux and macOS and after `MOVEFILE_WRITE_THROUGH` on Windows succeeds.
Finalizing recovery and temporary output reclamation by cancel/remove before finalization are implemented.
The subscriber's runtime sweep reclaims unreferenced roots and the temporary outputs of Canceled and Failed.
I/O failures during Finalizing placement and the sweep are retried by TaskDecoder with increasing intervals.
The retry wait responds to CancellationToken cancellation and shutdown.
Tests that interrupt processing around each storage commit of a publish and reopen the Store, and fault-injection tests of rename, metadata commit, and sweep failures, confirm reclamation and re-publication.
Tests that reopen the Store at each boundary of temporary output creation, file syncing, Finalizing recording, rename, and Completed recording, and fault-injection tests of placement, syncing, and temporary output reclamation failures, confirm recovery and existing-output protection.
Recovery after an OS crash is not confirmed.
On macOS, `HAVE_FULLFSYNC` is given to RocksDB with per-target `CXXFLAGS`, and it is confirmed that the aarch64-apple-darwin generated objects include a `fcntl(F_FULLFSYNC)` branch.
The non-replacing placement module passes type checks on aarch64-apple-darwin, x86_64-unknown-linux-gnu, and x86_64-pc-windows-msvc.
On macOS, the engine tests confirm existing-entry protection and successful file and parent-directory sync calls.
The x86_64-apple-darwin build, the full Linux and Windows engine builds and sync execution, and power-loss hardware tests are not confirmed.
For confirmed bugs, see [issues.md](../issues.md).
