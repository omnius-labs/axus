# Axus Terminology

## 1. About this document

This document is the sole source of truth for the Axus-specific vocabulary.
It covers definitions, notation, aliases, and the boundaries between adjacent terms.

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | Definitions of terms, notation, aliases, boundaries between adjacent terms, and the reasons for naming |
| [design.md](./design.md#11-division-of-responsibilities-among-documents) | Design rationale, invariants, implementation status, and deferred decisions |
| [issues.md](./issues.md) | Confirmed bugs in the code and their handling |

The map of the document family is held by [design.md](./design.md#11-division-of-responsibilities-among-documents).

## 2. Terminology

| Term | Scope | Identifier | External form | Refers to | Deprecated aliases |
| --- | --- | --- | --- | --- | --- |
| FileExchanger | file-transfer | `FileExchanger` | - | Component that uses NodeFinder's lookup results to establish a Session with a peer and carries blocks | - |
| FilePublisher | file-transfer | `FilePublisher` | - | Component that converts a local file into blocks and Merkle layers and publishes it | - |
| publish commit | file-transfer | `FilePublisherStore::commit` | - | Operation that finalizes the root hash of an uncommitted file and its stored blocks as a committed file | - |
| Store | file-transfer | `FilePublisherStore`, `FileSubscriberStore` | - | Component that mutates metadata and block bodies and manages the consistency of creating, finalizing, and deleting output files and their persistence | - |
| FileSubscriber | file-transfer | `FileSubscriber` | - | Component that gathers blocks from a root hash and decodes the file | - |
| output finalization | file-transfer | `Finalizing` → `Completed` | - | Operation that places and persists the decoded temporary output at the user's destination with a non-replacing rename and records Completed | - |
| MerkleLayer | file-transfer | `MerkleLayer` | - | Block content that stores the hash list of child blocks and the rank of those blocks | - |
| rank | file-transfer | `MerkleLayer.rank` | - | Level of a block in the Merkle structure; the file body blocks are rank 0 | - |
| root hash | file-transfer | `OmniHash` | - | Content identifier that is the hash of the topmost block | - |
| output reservation | file-transfer | - | - | Unique constraint on the subscription row that prevents other in-progress subscriptions from using the same output destination | - |
| choke | file-transfer | - | - | State of not responding to a peer's block requests; judged per node, and lifting it is called unchoke | - |
| status message | file-transfer | - | - | FileExchanger message that tells whether the remote's blocks are wanted and whether the remote is choked | - |
| do-not-have response | file-transfer | - | - | FileExchanger message that tells the requested hash's block is not held | - |
| committed file | file-transfer | `PublishedCommittedFile` | - | Publisher-side file whose root hash is finalized and which can be advertised to the network; identified by the pair of root hash and file name | - |
| uncommitted file | file-transfer | `PublishedUncommittedFile` | - | Publisher-side file from import until its root hash is finalized; identified by a temporary ID | - |
| temporary output | file-transfer | - | `.<entry name>.axus-<subscription ID>.part` | File created in the same directory as the output destination with a conventional name per subscription, into which the decoded result is written | - |
| Finalizing | file-transfer | `SubscribedFileStatus::Finalizing` | - | Subscription state from persisting the temporary output to begin finalization until reaching Completed or Failed | - |
| Task | file-transfer | `TaskDecoder`, `TaskEncoder` | - | Component that controls encoding/decoding, cancellation before finalization, and retries of sweep and output placement; distinct from the FileExchanger connection task | - |
| AssetKey | node-finder | `AssetKey` | - | Identifier whose location NodeFinder looks up | file reference |
| bootstrap node | node-finder | `bootstrap_node_profiles` | `p2p.bootstrap_nodes` | NodeProfile given by configuration and added to the known nodes at startup | - |
| location info | node-finder | `give_asset_key_locations` | - | Mapping of an AssetKey to the NodeProfiles of the nodes that advertise it | - |
| NodeFinder | node-finder | `NodeFinder` | - | Component that exchanges known nodes and looks up the location info of AssetKeys | - |
| known node | node-finder | `NodeFinderRepo` | - | Set of NodeProfiles saved as connection candidates | - |
| NodeProfile | node-finder | `NodeProfile` | `axus:node/` URI | Value binding a node's public key, reachable addresses, and update time with the node's own signature | - |
| reachable address | node-finder | `NodeProfile.addrs` | - | List of addresses carried in a NodeProfile at which the node can be reached | - |
| node ID | node-finder | `NodeProfile::id()` | - | SHA3-256 hash of the node's public key; used for Kadex distance calculation and Session duplicate resolution | `NodeProfile.id` |
| update time | node-finder | `NodeProfile::updated_time` | - | Time at which the NodeProfile was signed (unix seconds); decides which NodeProfile is newer for the same node ID | - |
| FileRef | profile-memo | `FileRef` | - | Pair of a name and a hash with which higher-level data references a published file | file body |
| memo | profile-memo | `MemoExchanger` | - | Lower-level mechanism that discovers, exchanges, and distributes Profiles | post body |
| Profile | profile-memo | `Profile` | - | Higher-level data representing a subject's public information and FileRefs to public data | - |
| rpf | rocketpack | `.rpf` | `.rpf` file | Schema file defining RocketPack types and field numbers | Rust type definition |
| Session | session | `Session` | - | Communication channel that carries the peer's proof, the connection direction, and a use | TCP connection |
| sweep | storage | - | - | Reclamation that reconciles RocksDB keys and temporary outputs against SQLite rows and deletes unreferenced ones | - |

## 3. Boundaries between adjacent terms

<a id="t-publisher-commit-vs-output-finalization"></a>
#### Publish commit and output finalization

**Boundary**
The publish commit turns blocks and a root hash into a committed file that can be advertised.
The output finalization places and persists the decoded result of a subscription at the filesystem output destination, and only then records Completed.
The DB commit is one step that makes up each of the two operations.

**What goes wrong if misread**
Treating the output finalization as only the DB commit of Completed returns success before the placement and directory persistence finish, and the output can be lost after a failure.

<a id="t-root-hash-vs-output-reservation"></a>
#### root hash and output reservation

**Boundary**
The root hash identifies content and is used to share blocks between subscriptions.
The output reservation prevents conflicts over the same filesystem entry.

**What goes wrong if misread**
Using the root hash as the reservation unit makes it impossible to subscribe to the same content at different destinations and also fails to prevent different content from being subscribed to the same destination.

<a id="t-node-profile-vs-session"></a>
#### NodeProfile and Session

**Boundary**
A NodeProfile binds a node's public key and its reachable addresses with the node's own signature and is used for lookup.
A Session is a communication channel that is handed to a use after verifying the peer's signature and agreeing on keys; about who the peer is, it guarantees only that the communication peer holds the private key for that public key.
Reconciling the public keys of the two is handled by [trust-security.md](./design/trust-security.md#node-id-is-derived-from-the-public-key).

**What goes wrong if misread**
Treating a successfully verified NodeProfile signature as proof of trust in the node, or as proof that the reachable address still belongs to the node, bypasses the outstanding trust contract that is confirmed in the Session.

<a id="t-asset-key-vs-file-ref"></a>
#### AssetKey and FileRef

**Boundary**
An AssetKey is a key whose location is looked up on the network.
A FileRef is a value handed to higher-level data that references content by a display name and a hash; it carries no lookup results.

**What goes wrong if misread**
Treating a FileRef as a lookup key leaks NodeFinder's lookup contract into the higher-level representation that includes the display name.

<a id="t-file-ref-vs-root-hash"></a>
#### FileRef and root hash

**Boundary**
The root hash is a hash that identifies only content.
A FileRef is a value with which higher-level data references content by name.

**What goes wrong if misread**
Treating the root hash alone as a FileRef loses the display name and classification that the higher-level service should keep.

<a id="t-profile-vs-memo"></a>
#### Profile and memo

**Boundary**
A Profile is a payload representing public information and a set of FileRefs.
A memo is a lower-level mechanism that discovers, exchanges, and distributes that payload, and does not interpret the post body or attachments.

**What goes wrong if misread**
If the memo interprets the Profile content, changes to the higher-level data model ripple into the discovery and distribution protocol.
