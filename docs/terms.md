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
| Library root | daemon-api | `LibraryRoot` | `storage.library_roots` | Directory configured as readable by a publication, identified in the API by an ID | - |
| Owner | daemon-api | `Owner` | - | The single person account that operates the daemon | `admin`, `user` |
| Owner session | daemon-api | `OwnerSession` | `__Host-axus_session`, `axus_session`, `Authorization: Bearer` | Server-side row that proves the owner is logged in | `login state` |
| Publication | daemon-api | `Publication` | `/publications/{id}` | The API's view of one import, from the declaration of its size and source to the committed file its ID resolves to | - |
| Received offset | daemon-api | `Publication.received_bytes` | `receivedBytes` | Number of content bytes accepted for a publication; the staged file's length is reconciled at startup | - |
| Source | daemon-api | `PublicationSource` | `source` | Where a publication reads its content: a staged upload or a library file | - |
| Staged file | daemon-api | - | - | Copy of uploaded content under `state_dir/uploads` that the encoder reads and the daemon deletes | - |
| Download directory | daemon-api | `storage.download_dir` | `storage.download_dir` | Directory in which the daemon places subscription outputs | - |
| Subscription | daemon-api | `Subscription` | `/subscriptions/{id}` | The API's view of the work that fetches a root hash and places its output under the download directory | - |
| cancel message | file-transfer | `Cancel` | - | FileExchanger message that retracts requested hashes whose blocks are no longer needed | - |
| choke | file-transfer | - | - | State of not responding to a peer's block requests; judged per node, and lifting it is called unchoke | - |
| fetch throughput | file-transfer | - | - | Received bytes per period of the blocks whose content hash matched, recorded per node ID and used to order the dialing candidates and to set the requests awaiting responses | - |
| FileExchanger | file-transfer | `FileExchanger` | - | Component that uses NodeFinder's lookup results to establish a Session with a peer and carries blocks | - |
| FilePublisher | file-transfer | `FilePublisher` | - | Component that converts a local file into blocks and Merkle layers and publishes it | - |
| Store | file-transfer | `FilePublisherStore`, `FileSubscriberStore` | - | Component that mutates metadata and block bodies and manages the consistency of creating, finalizing, and deleting output files and their persistence | - |
| publish commit | file-transfer | `FilePublisherStore::commit` | - | Operation that finalizes the root hash of an uncommitted file and its stored blocks as a committed file | - |
| FileSubscriber | file-transfer | `FileSubscriber` | - | Component that gathers blocks from a root hash and decodes the file | - |
| output finalization | file-transfer | `Finalizing` → `Completed` | - | Operation that places and persists the decoded temporary output at the user's destination with a non-replacing rename and records Completed | - |
| presence bitmap | file-transfer | `HaveMap` | - | FileExchanger message that starts the answer to a request and tells, one bit per requested hash, which blocks are held | - |
| MerkleLayer | file-transfer | `MerkleLayer` | - | Block content that stores the hash list of child blocks and the rank of those blocks | - |
| rank | file-transfer | `MerkleLayer.rank` | - | Level of a block in the Merkle structure; the file body blocks are rank 0 | - |
| root hash | file-transfer | `OmniHash` | - | Content identifier that is the hash of the topmost block | - |
| output reservation | file-transfer | - | - | Unique constraint on the subscription row that prevents other in-progress subscriptions from using the same output destination | - |
| committed file | file-transfer | `PublishedCommittedFile` | - | Publisher-side file whose root hash is finalized and which can be advertised to the network; identified by the pair of root hash and file name | - |
| uncommitted file | file-transfer | `PublishedUncommittedFile` | - | Publisher-side file from import until its root hash is finalized; identified by a temporary ID | - |
| status message | file-transfer | - | - | FileExchanger message that tells whether the remote's blocks are wanted and whether the remote is choked | - |
| Finalizing | file-transfer | `SubscribedFileStatus::Finalizing` | - | Subscription state from persisting the temporary output to begin finalization until reaching Completed or Failed | - |
| tail duplication | file-transfer | - | - | Assignment of the same block to two Sessions when few blocks remain, where the first block stored completes it | endgame |
| Task | file-transfer | `TaskDecoder`, `TaskEncoder` | - | Component that controls encoding/decoding, cancellation before finalization, and retries of sweep and output placement; distinct from the FileExchanger connection task | - |
| temporary output | file-transfer | - | `.<entry name>.axus-<subscription ID>.part` | File created in the same directory as the output destination with a conventional name per subscription, into which the decoded result is written | - |
| location info | node-finder | `AnnounceLocations` | - | Mapping of an AssetKey to the NodeProfiles of the nodes that advertise it | - |
| AssetKey | node-finder | `AssetKey` | - | Identifier whose location NodeFinder looks up | file reference |
| bootstrap node | node-finder | `bootstrap_node_profiles` | `p2p.bootstrap_nodes` | NodeProfile given by configuration and added to the known nodes at startup | - |
| NodeFinder | node-finder | `NodeFinder` | - | Component that exchanges known nodes and looks up the location info of AssetKeys | - |
| known node | node-finder | `NodeFinderRepo` | - | Set of NodeProfiles saved as connection candidates | - |
| NodeProfile | node-finder | `NodeProfile` | `axus:node/` URI | Value binding a node's public key, reachable addresses, and update time with the node's own signature | - |
| reachable address | node-finder | `NodeProfile.addrs` | - | List of addresses carried in a NodeProfile at which the node can be reached | - |
| node ID | node-finder | `NodeProfile::id()` | - | SHA3-256 hash of the node's public key; used for Kadex distance calculation and Session duplicate resolution | `NodeProfile.id` |
| update time | node-finder | `NodeProfile::updated_time` | - | Time at which the NodeProfile was signed (unix seconds); decides which NodeProfile is newer for the same node ID | - |
| FileRef | profile | `FileRef` | - | Pair of a name and a root hash with which a Profile references a published file | file body |
| message | profile | `ProfileMessage` | - | Bulletin-board entry in a Profile, with a posted time and an inline body or a FileRef | post |
| Profile | profile | `Profile` | - | Signed public statement of one identity, identified by its Profile ID: its FileRefs, trust and block lists, and messages | - |
| Profile ID | profile | `Profile::id` | `name@u<base64url>` | Immutable identity of a Profile; `name` is 1–32 bytes of `[A-Za-z0-9._-]` and the suffix is the `OmniSigner` display form of the DER public key (`u` followed by the base64url encoding of its SHA3-256) | - |
| active Profile | profile | `AxusService::active_profile` | - | The Profile that authors new publications and is the root of the trust evaluation | - |
| profile key | profile | - | - | Ed25519 key pair that signs one Profile, distinct from the node key | profile signing key |
| trust list | profile | `Profile::trusted` | - | Profile IDs the Profile trusts | allow list |
| block list | profile | `Profile::blocked` | - | Profile IDs the Profile blocks | deny list, distrust list |
| ProfileExchanger | profile-exchanger | `ProfileExchanger` | - | Component that looks up, exchanges, and refreshes Profiles | MemoExchanger, memo |
| Web of Trust | web-of-trust | - | - | Local evaluation of the trust and block lists over signed Profiles | WoT |
| F2F | web-of-trust | - | - | Network that admits only mutually trusted peers at connection time; not adopted | friend-to-friend |
| rpf | rocketpack | `.rpf` | `.rpf` file | Schema file defining RocketPack types and field numbers | Rust type definition |
| Session | session | `Session` | - | Communication channel that carries the peer's proof, the connection direction, and a use | TCP connection |
| sweep | storage | - | - | Reclamation that reconciles RocksDB keys and temporary outputs against SQLite rows and deletes unreferenced ones | - |

## 3. Boundaries between adjacent terms

<a id="t-publication-vs-committed-file"></a>
#### Publication and committed file

**Boundary**
A publication is the API's identity of one import; it exists before any byte is received, and its ID resolves to the committed file after the commit.
A committed file is the network's unit, identified by the pair of root hash and file name, and it is what a subscription names.

**What goes wrong if misread**
Using a publication ID as share text gives the recipient an identifier that their node cannot look up, because the network addresses content by root hash and name.

<a id="t-session-vs-owner-session"></a>
#### Session and owner session

**Boundary**
A Session is the P2P communication channel between two nodes, established by the transport and handed to a use.
An owner session is a row in the daemon's database that proves the operator logged in; it never travels between nodes.

**What goes wrong if misread**
Using the word session for both makes a client's login token look like something the P2P layer can establish or verify, and a code path that expects one can accept the other's identifier.

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

<a id="t-nodeprofile-vs-profile"></a>
#### NodeProfile and Profile

**Boundary**
A NodeProfile is the transport identity of a node: its public key, reachable addresses, and update time, signed with the node key.
A Profile is a content identity: its FileRefs, trust and block lists, and messages, signed with the profile key, and its identity is independent of the node that serves it.

**What goes wrong if misread**
Treating a NodeProfile as an author, or a Profile as a reachable peer, makes the trust lists look like connection permissions (F2F) and makes a node's address part of the author's identity.

<a id="t-f2f-vs-wot"></a>
#### F2F and Web of Trust

**Boundary**
F2F admits only mutually trusted peers at connection time.
The Web of Trust evaluates signed content and never refuses a connection.

**What goes wrong if misread**
Gate the Session by the trust list and the design becomes F2F: a new peer cannot exchange a Profile, and the address becomes part of the trust decision.

<a id="t-profile-vs-profile-exchanger"></a>
#### Profile and ProfileExchanger

**Boundary**
A Profile is a signed body with its lists and messages.
A ProfileExchanger finds and transfers the body and does not interpret it.

**What goes wrong if misread**
Storing trust in the exchanger, or letting it interpret the lists, duplicates the evaluation and makes a transport component the owner of a trust rule.
