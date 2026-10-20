# Design document and daemon audit review

## 1. About this review

This is an audit review that checked, by reading through the implementation, whether the design documents diverge from the daemon implementation and whether any judgment is missing from the design itself.

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | The scope, viewpoints, findings, and what was verified and found sound in this review |
| [issues.md](../issues.md) | Transcription target for findings of type bug and improvement proposal |
| [terms.md](../terms.md) | Transcription target for findings of type terminology |
| [session.md](../design/session.md), [trust-security.md](../design/trust-security.md), [file-transfer.md](../design/file-transfer.md), [daemon-api.md](../design/daemon-api.md), [rocketpack.md](../design/rocketpack.md) | Transcription targets for findings of type design point |

The map of the document family is held by [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Review date and targets

1. Review date: 2026-09-25
2. Type: audit review
3. Target commits: `59054a2` (core-rs: `5a92929`)
4. Reviewer: Claude Code (AI agent)

## 2. Scope and viewpoints

### 2.1 What was reviewed

All of the `docs/` design documents, terminology, and issue list, and `daemon/modules/engine/src`'s Session, NodeFinder, FileExchanger, FilePublisher, FileSubscriber, and block storage, and `daemon/entrypoints/daemon/src`.
Also read the core-rs design document for the RocketPack compiler that the design documents assume, the RocketPack and secure connection sections of DESIGN.md, and the signing in `OmniSigner` and `OmniSecureStream`.
The scope was cut at the implementation of the concerns the design documents own.

Findings R-1, R-5, and R-8 were reproduced with temporary tests, which were then removed.
All 45 existing engine tests were confirmed to pass.

### 2.2 What was not reviewed

- The `daemon/entrypoints/interface` artifacts are generated and not hand-edited, so they were out of scope
- The core-rs implementation was out of scope except for the signing and decoder portions Axus uses
- Compliance with `docs/coding/rust.md` was out of scope because the lint is the source of truth
- The UPnP and SOCKS5 paths could not be checked in the time available because there are no physical router and proxy
- FileExchanger's `task_accepter.rs` and `session_status.rs` were skimmed because there is no block exchange protocol and no processing body
- Performance and resource consumption were not measured

### 2.3 Viewpoints

- Does the description in the design documents match the implementation?
- Do the boundaries that receive external input have deadlines and size limits?
- What does the authentication guarantee, and what does it not?
- Can a process spanning the two storages recover if it ends midway?
- Does the flow the design documents promise have a data origin in the implementation?

## 3. Overall assessment

NodeFinder lookup and Session duplicate resolution work as designed, and apart from the known bugs the current state is usable.
On the other hand, Session has no connection deadline (R-1), and authentication is not bound to the connection (R-2), so both must be fixed before the daemon goes on an untrusted network.
The publish path lacks the recovery procedure for a commit spanning the two storages (R-5) and must be fixed before wiring FileExchanger.

## 4. Findings

| ID | Summary | Type | Confidence | Handling |
| --- | --- | --- | --- | --- |
| [R-1](#r-1) | Non-responding connections stop Session acceptance and dialing | Bug | High | Filed in issues.md |
| [R-2](#r-2) | Session authentication is not bound to the connection | Design point | High | Transcribed to session.md's deferred |
| [R-3](#r-3) | The reachable-address authenticity deferred rests on an old premise and a wrong current state | Design point | High | Transcribed to trust-security.md's deferred |
| [R-4](#r-4) | Received message size is limited only by the frame limit | Bug | High | Filed in issues.md |
| [R-5](#r-5) | A file that ended mid-commit cannot be published again | Bug | High | Filed in issues.md |
| [R-6](#r-6) | The location that checks a block's hash is undecided | Design point | High | Transcribed to file-transfer.md's deferred |
| [R-7](#r-7) | The publisher side has no way to find a connect target | Design point | High | Transcribed to file-transfer.md's deferred |
| [R-8](#r-8) | Kadex::find misses close nodes when 2 or more are requested | Bug | High | Filed in issues.md |
| [R-9](#r-9) | REST API reach and authentication are undecided | Design point | High | Transcribed to daemon-api.md's deferred |
| [R-10](#r-10) | rocketpack.md copies the old core-rs compiler specification | Design point | High | Transcribed to rocketpack.md's deferred |
| [R-11](#r-11) | The deprecated aliases of the terminology contain other terms, and key terms are missing | Terminology | High | Transcribed to terms.md's term list |
| [R-12](#r-12) | FileExchanger's startup failure is swallowed | Improvement proposal | High | Filed in issues.md |
| [R-13](#r-13) | The multiple-startup lock lives in the configuration directory instead of the state directory | Improvement proposal | High | Filed in issues.md |

R-1 and R-4 both come from the absence of deadlines and limits at the Session and NodeFinder input boundaries, and are most efficiently fixed together.
R-2 and R-3 come from the same cause, and the R-3 judgment depends on the R-2 judgment.
R-6 and R-7 must both be decided before defining the block exchange protocol.

<a id="r-1"></a>
### R-1. Non-responding connections stop Session acceptance and dialing

**Type: bug** / **Confidence: high**

**Location**
[accepter.rs:76](../../daemon/modules/engine/src/core/session/accepter.rs#L76) starts only three accept workers, and each worker waits for the handshake of a received connection with an unbounded reception from [accepter.rs:168](../../daemon/modules/engine/src/core/session/accepter.rs#L168) onward.
The dialing side's [connector.rs:35](../../daemon/modules/engine/src/core/session/connector.rs#L35) also has no deadline.

**What is wrong**
A pre-authentication peer only needs to open three connections that send nothing for that node to stop accepting new Sessions.

**Evidence**
With in-memory connections, three unresponsive connections were kept open and `SessionConnector::connect` was called; a temporary test confirmed it does not complete in 3 seconds.
That the dialing side stalls at an unresponsive reachable address was confirmed by reading the code, not by execution.

**Impact**
NodeFinder acceptance stops, and dialing also stops when an unresponsive reachable address is mixed into the known nodes.

**Handling**
Filed in issues.md as session-handshake-without-deadline.

<a id="r-2"></a>
### R-2. Session authentication is not bound to the connection

**Type: design point** / **Confidence: high**

**Location**
[accepter.rs:182](../../daemon/modules/engine/src/core/session/accepter.rs#L182) and [connector.rs:45](../../daemon/modules/engine/src/core/session/connector.rs#L45) sign only a 32-byte nonce received from the peer.

```rust
let send_signature = self.signer.sign(&receive_challenge_message.nonce)?;
```

**What is wrong**
Because the signature is bound to neither the TCP connection nor the handshake content, when a third party relays the handshake, both ends authenticate each other and communicate through the third party.
Also, a pre-authentication peer can make any node sign any 32 bytes.

**Evidence**
Confirmed in the code that `OmniSigner::sign` signs a received byte string with Ed25519 as-is, and that the V1 handshake has no key agreement and no use prefix.
Because core-rs's `OmniSecureStream` signs a 32-byte SHA3-256 of its own key-agreement public key with the same kind of key, answering both with the same key lets the signatures be reused.
Relay and impersonation were confirmed by tracing the procedure, not by execution.

**Impact**
Because NodeProfile reachable addresses are unsigned, a third party that can distribute fake reachable addresses can get between NodeFinder communication today.
Choosing secure channel candidate 1 leads to impersonation.

**Handling**
Transcribed to session.md's pending "Session secure channel".

<a id="r-3"></a>
### R-3. The reachable-address authenticity pending has an old premise and a wrong current state

**Type: design point** / **Confidence: high**

**Location**
trust-security.md's pending "authenticity of the NodeProfile reachable address".
It stated the current state as "impersonation cannot hold with a fake reachable address" and the reason for not deciding as "multi-hop lookup is not running in the daemon".

**What is wrong**
With the R-2 relay, a fake reachable address can get between communication, so the current-state description is wrong.
Multi-hop lookup runs in the three-node integration test at [service.rs:246](../../daemon/modules/engine/src/service.rs#L246), so the reason for not deciding no longer holds.

**Evidence**
Same as R-2.
The integration test passing was confirmed by running it.

**Impact**
The pending's decision condition stays unobservable, and the priority of the judgment is misjudged.

**Handling**
Transcribed to trust-security.md's pending "authenticity of the NodeProfile reachable address".

<a id="r-4"></a>
### R-4. Received message size is limited only by the frame limit

**Type: bug** / **Confidence: high**

**Location**
The 64 MiB at [framed.rs:9](../../daemon/modules/engine/src/base/connection/stream/framed.rs#L9) is the common limit for all messages, and [task_communicator.rs:454](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L454) and others read the element count without a limit.

**What is wrong**
Even a handshake message before authentication is read up to 64 MiB, and the number of NodeProfiles attached to one location entry has no limit.

**Evidence**
Confirmed in the code.
Also confirmed that core-rs's decoder rejects an element count exceeding the remaining byte count, so the allocation stays proportional to the frame size.
Actual memory consumption was not measured.

**Impact**
The message-size limit §3 of trust-security.md demands is satisfied only by the frame limit.

**Handling**
Filed in issues.md as unbounded-message-sizes.

<a id="r-5"></a>
### R-5. A file that ended mid-commit cannot be published again

**Type: bug** / **Confidence: high**

**Location**
[task_encoder.rs:305](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L305) renames a RocksDB key per block, and [task_encoder.rs:309](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L309) commits to SQLite.

**What is wrong**
If the process exits between the two, the encoding after a restart fails with the rename's `AlreadyExists`, and the file stays Failed.

**Evidence**
With a storage in which a committed key was placed first, the same file was published, and a temporary test confirmed it becomes Failed with the reason `AlreadyExists`.
The process exit itself was not performed.

**Impact**
A file whose publish processing stopped midway is not published until the user deletes and re-adds it, and unreferenced blocks remain.

**Handling**
Filed in issues.md as publisher-commit-not-crash-safe.

<a id="r-6"></a>
### R-6. The location that checks a block's hash is undecided

**Type: design point** / **Confidence: high**

**Location**
`write_block` at [subscriber.rs:123](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber.rs#L123) saves the received value at [subscriber.rs:130](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber.rs#L130) without checking it.

**What is wrong**
file-transfer.md says the content hash is verified before the commit, but whether to check at the save entry or in the protocol layer is undecided.

**Evidence**
Confirmed in the code.

**Impact**
Checking only in the protocol layer leaves room for a path calling `write_block` to forget the check as the paths grow.

**Handling**
Transcribed to file-transfer.md's pending "location that checks a block's hash".

<a id="r-7"></a>
### R-7. The publisher side has no way to find a connect target

**Type: design point** / **Confidence: high**

**Location**
[task_connector.rs:196](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L196) calls `find_node_profile` even for a publish connection, and [node_finder.rs:215](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L215) returns only the location info of providing nodes.

**What is wrong**
file-transfer.md said the publisher side connects to requesting nodes, but since NodeFinder's want does not carry the requester's NodeProfile, the publisher side has no way to know the requesting nodes.

**Evidence**
Confirmed in the code that DataMessage's want is only a list of `AssetKey`s.

**Impact**
The publish connection task connects to other nodes providing the same file.

**Handling**
Transcribed to file-transfer.md's pending "connect target of the publisher side".

<a id="r-8"></a>
### R-8. Kadex::find misses close nodes when 2 or more are requested

**Type: bug** / **Confidence: high**

**Location**
The carry-down at [kadx.rs:42](../../daemon/modules/engine/src/core/negotiator/node/kadx.rs#L42) stops one before the last.

**What is wrong**
If the candidates are not sorted by closeness, requesting 3 returns only 2.

**Evidence**
Confirmed with a temporary test that passes 3 candidates in far-to-close order and requests 3, receiving only 2.

**Impact**
Not visible because the current callers request only 1.

**Handling**
Filed in issues.md as kadex-find-drops-candidates.

<a id="r-9"></a>
### R-9. REST API reach and authentication are undecided

**Type: design point** / **Confidence: high**

**Location**
The health at [openapi.yaml:25](../../daemon/openapi.yaml#L25) has `security: []`, and [server.rs:37](../../daemon/entrypoints/daemon/src/server.rs#L37) listens on the configured address as-is.

**What is wrong**
The REST API is said to be for local clients, but whether the daemon enforces the premise or protects with authentication is undecided.

**Evidence**
Confirmed in the code and OpenAPI.

**Impact**
When a state-changing endpoint is added, a daemon listening on a non-loopback address can be operated by anyone.

**Handling**
Transcribed to daemon-api.md's pending "REST API reach and authentication".

<a id="r-10"></a>
### R-10. rocketpack.md copies the old core-rs compiler specification

**Type: design point** / **Confidence: high**

**Location**
The `rocketpack.yaml` explanation in rocketpack.md §3 and the pending "handling of core-rs types referenced from the rpf".

**What is wrong**
The statement that `targets`' `pattern` selects the generation targets and creates one Rust file from one rpf diverges from core-rs's rocketpack-compiler.md §7.
The pending's options also stood on the old premise of referencing Rust types that implement `RocketPackStruct` as external types.

**Evidence**
Confirmed with core-rs `5a92929`'s rocketpack-compiler.md §7 and §9, and omnikit's `rocketpack.yaml` and `omni_hash.rpf`.

**Impact**
When deciding the pending, the options chosen from are ones that do not exist.

**Handling**
Transcribed to rocketpack.md's pending "handling of core-rs types referenced from the rpf".

<a id="r-11"></a>
### R-11. The deprecated aliases of the terminology contain other terms, and key terms are missing

**Type: terminology** / **Confidence: high**

**Location**
The term list of terms.md.
The aliases contain: root hash as rank's alias, FileRef as root hash's alias, and memo as Profile's alias, all of which are terms in the same table.

**What is wrong**
Putting an adjacent term in the deprecated aliases reads as "do not use this term".
NodeFinder, FileExchanger, FilePublisher, FileSubscriber, MerkleLayer, known node, bootstrap node, location info, and reachable address, which the design documents use, are not in the table.

**Evidence**
Confirmed by reconciling terms.md against the design documents.

**Impact**
Looking a term up finds no definition.

**Handling**
Transcribed to terms.md's term list (fixed in the same change as this review).

<a id="r-12"></a>
### R-12. FileExchanger's startup failure is swallowed

**Type: improvement proposal** / **Confidence: high**

**Location**
[file_exchanger.rs:93](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L93) discards the `Result` of `start`, and the `#[allow(unused)]` at [file_exchanger.rs:57](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L57) suppresses the warning.

**What is wrong**
FileExchanger is created even when its storage cannot be opened.

**Evidence**
Confirmed in the code.

**Impact**
Not visible because FileExchanger is not wired yet.

**Handling**
Filed in issues.md as file-exchanger-drops-start-error.

<a id="r-13"></a>
### R-13. The multiple-startup lock is in the configuration directory, not the state directory

**Type: improvement proposal** / **Confidence: high**

**Location**
[executor.rs:85](../../daemon/entrypoints/daemon/src/executor.rs#L85) locks the `axus.lock` in the configuration directory.

**What is wrong**
A second daemon can be started from another configuration directory pointing to the same `state_dir`.

**Evidence**
Confirmed in the code.
Two daemons were not actually started.

**Impact**
Two nodes with the same signing key run, and the known-node writes mix.

**Handling**
Filed in issues.md as daemon-lock-not-on-state-dir.

## 5. Verified and found sound

- The suspicion that two mutually connecting nodes keep different Sessions was dismissed because the rule at [task_communicator.rs:159](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L159) makes both sides choose the same Session and the integration test passes.
- The suspicion that a known node's NodeProfile is a different person from the Session peer was dismissed because the NodeFinder handshake reconciles the cert's public key. The R-2 relay, however, remains separately.
- The suspicion that `Vec::with_capacity` at decode allocates huge memory from the element count alone was confirmed to stay within the frame size because core-rs's decoder rejects an element count exceeding the remaining byte count.
- The suspicion that a RocksDB rename breaks midway was dismissed because `rename_key` swaps a `names` entry in one transaction.
- The suspicion that the rank meaning of MerkleLayer diverges between the encoding and decoding sides was dismissed because the multi-stage round-trip test passes.
- The suspicion that the signing key file is read mid-write was dismissed because the key is written to a temporary file and then renamed.

## 6. Follow-up

- 2026-10-06: Fixed R-8, extending `Kadex::find`'s carry-down to the end.
- 2026-10-06: Fixed R-12 so `FileExchanger::new` returns the `start` failure and stops the started components then.
- 2026-10-06: Fixed R-13 so the daemon's lock is moved from the configuration directory to `axus.lock` in `core.state_dir`.
