# Design of Profile

## 1. About this document

This document covers the identity of a Profile, the signed body it carries, the FileRefs and messages it lists, and the rules that decide which snapshot is current.
Lookup and exchange belong to [profile-exchanger.md](./profile-exchanger.md#1-about-this-document), and the evaluation of the trust and block lists to [web-of-trust.md](./web-of-trust.md#1-about-this-document).
This document assumes the vocabulary of [terms.md](../terms.md). Term definitions are not given here.

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, boundaries with other concerns, cross-cutting dependencies |
| This document | Profile identity, the signed body, the lists it holds, and the update rules |
| [storage.md](./storage.md#48-profile-storage) | The local layout of Profile keys, signed bodies, and the active flag |
| [profile-exchanger.md](./profile-exchanger.md#1-about-this-document) | Lookup, exchange, and refresh of Profiles |
| [web-of-trust.md](./web-of-trust.md#1-about-this-document) | Evaluation of the trust and block lists |
| [file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries) | Publishing and subscribing to the files a FileRef names |
| [trust-security.md](./trust-security.md#2-responsibilities-and-boundaries) | What the Session authenticates and the identity of the node |
| [daemon-api.md](./daemon-api.md#1-about-this-document) | The REST resources and their authentication |
| [terms.md](../terms.md) | Term definitions |

### 1.2 Tenses used in this document

The body describes the completed design.
The implementation status and remaining work are consolidated in §9.
To find out what runs now, read §9 first.

## 2. Responsibilities and boundaries

The Profile layer owns the signed statement of one identity and the rules that decide which statement is current.
The signature establishes that the holder of the profile key wrote the statement.
A Profile is not a transport identity.
The Session authenticates the node through the NodeProfile and the node key ([trust-security.md](./trust-security.md#nodeprofile-binds-reachable-addresses-with-the-nodes-own-signature)); a Profile is content that outlives the addresses of any one node, and in this version only the node that owns it serves it ([profile-exchanger.md](./profile-exchanger.md#32-locations)).
This separation is the basis for holding several Profiles on one node without tying any of them to the node's address.

A Profile does not carry the file bodies, the transfer state, or the block layout.
It names files through FileRefs, and the file protocol owns how a FileRef's root hash turns into blocks ([file-transfer.md](./file-transfer.md#2-responsibilities-and-boundaries)).

## 3. Profile identity

### 3.1 Profile ID

A Profile ID is the readable handle defined in [terms.md](../terms.md), with a display part and a key-derived suffix.
The form and name rule are defined in [terms.md](../terms.md#2-terminology), and the ID is at most 80 bytes ([§7](#7-limits)).
The ID is immutable and is part of the signed body: renaming a Profile creates a different ID and therefore a different identity, and changing the form after profiles exist would break the trust lists and the ProfileExchanger AssetKey.

The suffix exists because a Profile is addressed by a string that anyone can choose.
Without a key-derived part, two Profiles could claim the same display string and the receiver could not tell them apart before fetching both.
With it, a receiver recomputes the suffix from the public key in the signed body and rejects a body whose key does not produce the ID.

The ID is also the trust unit: the trust and block lists name Profile IDs, and a file or a message is attributed to the Profile that lists it.

### 3.2 Profile key

Each Profile has its own Ed25519 key pair and signs its body with it.
The key is generated when the Profile is created and stored in the state directory beside the node key.
The key and body layout is in [storage.md §4.8](./storage.md#48-profile-storage).

The profile key is not the node key.
The node key proves reachability ([trust-security.md](./trust-security.md#node-id-is-derived-from-the-public-key)) and is bound to the node ID, while the profile key proves authorship and is bound to the Profile ID.
Sharing one key would tie every identity of a node to its network address, and rotating the transport key would change the author of every past statement.
Keeping them apart lets one node hold several identities and keeps a Profile's identity independent of the node's address.

### 3.3 Several Profiles on one node

A node stores any number of Profiles it owns and serves all of them.
The owner selects one active Profile ([terms.md](../terms.md#2-terminology)).
It authors new publications and is the root of the trust evaluation ([web-of-trust.md](./web-of-trust.md#41-the-root)).

The other local Profiles are not trusted by the active Profile unless the active Profile's trust list says so.
Selecting a different Profile therefore changes the trust result, and the client must show which Profile is active.

## 4. The signed body

### 4.1 Fields

The signed body carries the Profile ID, the public key, a version, an update time, the trust list, the block list, the FileRef list, the message list, and the signature.
The signature covers a RocketPack encoding of everything except the signature, with a fixed prefix that separates the use from the NodeProfile signature.

The key is carried in the body rather than derived from the ID, because the receiver needs it to verify the signature before it can trust anything else, and because the suffix is shorter than a full key.

### 4.2 FileRefs

A FileRef names a published file by its name and root hash ([terms.md](../terms.md#t-file-ref-vs-root-hash)).
The FileRef list is the set of files the Profile claims to publish.

The Profile's signature covers the list, so a FileRef is signed by inclusion and carries no signature of its own.
Adding or removing a file is an update of the Profile, and a receiver that has the current Profile knows the exact set the author claims.
A file that no Profile lists has no author, and the client shows it as unattributed rather than guessing who published it.

A FileRef does not carry the size or the block size of the file.
Those are learned from the file itself during subscription, and repeating them in the Profile would only create a second value to keep consistent.

### 4.3 Trust list and block list

The trust list names the Profile IDs this Profile trusts, and the block list names the Profile IDs it blocks.
The two lists are data: this document owns their format and the update rules, and [web-of-trust.md](./web-of-trust.md#4-evaluation) owns what a receiver computes from them.

A Profile cannot trust or block itself; the encoder rejects it.
An entry appears in at most one list, and the encoder rejects a body that places the same Profile ID in both lists.
Block precedence applies across Profiles at evaluation time ([web-of-trust.md](./web-of-trust.md#44-block-precedence)).

### 4.4 Messages

A message carries a posted time and a body.
The body is inline up to 64 KiB.
A larger body or an attachment is published as a file, and the message holds a FileRef to it.

The inline body exists because a small post must not cost a lookup and a file fetch.
The Profile also carries the message list in the same signed snapshot, so a reader gets the posts and the author's statement about them in one exchange.

The message list is ordered by posted time.
The Profile holds a bounded number of messages, and older ones are removed by the author when the bound is reached; the bound is in §7.

## 5. Update rules

### 5.1 Version and update time

Every update increments the version and sets the update time, and the whole body is signed again.
A receiver treats a snapshot as newer when its update time is later; when the times are equal, the higher version is newer.

The update time is primary because it is the same rule the NodeProfile uses, and because a restored backup can carry a lower version with a correct later time.
The version breaks ties between two updates in the same second and lets the receiver detect a re-signed body with the same time.

### 5.2 Replacement and rollback

A receiver replaces the stored snapshot only with a newer one.
An older snapshot is ignored, so a redelivered old Profile cannot roll the lists back.

An update time more than 5 minutes ahead of the receive time is rejected, with the same tolerance the NodeProfile uses ([trust-security.md](./trust-security.md#nodeprofile-binds-reachable-addresses-with-the-nodes-own-signature)).
Without the limit, a forged far-future time would win every comparison for years.

An honest owner whose clock moves back must wait until the stored update time is no longer ahead before the network accepts the next snapshot.
This is the same cost the NodeProfile accepts, and keeping both to one rule avoids a second clock premise.

### 5.3 Equivocation

When two different bodies carry the same update time and the same version, the receiver keeps the first and logs the second.
The receiver cannot tell which one the owner meant, and choosing the later arrival would let a relaying node decide.

### 5.4 Local updates

An owner update takes a complete draft of the FileRefs, trust and block lists, and messages, increments the version, sets the update time to the current time, signs the whole body, and stores it.
If the current time is earlier than the stored update time, the update is rejected and the owner must wait.
Updates of one Profile are serialized so two updates cannot sign from the same snapshot.

## 6. Publishing through a Profile

At startup, a stored key without a body is recovered as an empty Profile ([storage.md §4.8](./storage.md#48-profile-storage)).
If no Profile exists after that, the daemon creates `Anonymous`, generates its key, signs an empty snapshot, and makes it active. The daemon does this so a node always has an author for new publications and a trust root.
The owner can create other Profiles later and select the active one.

A publication selects the active Profile.
At the publish commit the daemon appends a FileRef to the active Profile's list and signs a new snapshot; removing a committed file removes the FileRef and signs again.
The FileExchanger advertises the committed files it stores and the root hashes of the subscriptions that provide blocks ([file-transfer.md §5.1](./file-transfer.md#51-participating-nodes-and-advertisement)).
Advertisement is availability, not authorship: a relay can be found without claiming the FileRef, and the FileRef list decides who claims the file.

A Profile may list a FileRef for a file its node does not hold.
The FileRef is a claim of authorship, not a promise of hosting, and the lookup finds whichever node holds the blocks.
When several Profiles list the same file, the client shows every claimant and the trust state of each ([web-of-trust.md](./web-of-trust.md#51-files)).

## 7. Limits

| Item | Limit |
| --- | --- |
| Profile ID | 80 bytes |
| Trust list entries | 256 |
| Block list entries | 256 |
| FileRefs | 1024 |
| Messages | 256 |
| Inline message body | 64 KiB |
| Encoded Profile | 1 MiB |

The limits keep one Profile inside one ProfileExchanger frame ([profile-exchanger.md](./profile-exchanger.md#42-frame-and-messages)) and bound the work of a receiver.
They are initial values; measurement of real Profiles decides whether they move.

## 8. Design decisions

### 8.1 Decided

#### A Profile is a mutable signed snapshot

**Decision**
A Profile is one signed body with a version and an update time, and every list change produces a new signed snapshot that replaces the previous one.

**Rationale**
The lists change often, and a mutable snapshot is the simplest object a receiver can store and compare.
An immutable chain would make history verifiable, but finding the latest version and reclaiming old bodies would add a second lookup problem.

**Rejected**
An immutable chain of Profile versions was considered and rejected because it needs a rule for finding the latest and a store for the history, and the trust evaluation only needs the current lists.

#### The profile key is separate from the node key

**Decision**
Each Profile has its own key pair, and the node key is never used to sign a Profile.

**Rationale**
The two keys answer different questions: the node key proves that a peer is reachable at an address, and the profile key proves who wrote a statement.
One node holds several Profiles, and a Profile's identity does not depend on its server, so neither key can stand for the other.

**Rejected**
Reusing the node key was considered and rejected because it would bind every identity of a node to its address and would change the author of past statements when the transport key changes.

#### The Profile ID carries a key-derived suffix and is immutable

**Decision**
A Profile ID carries the display part and a suffix derived from the profile key, and the whole ID is signed and never changes.

**Rationale**
A free-form name could be claimed by two keys, and a receiver could not tell them apart before fetching both.
A key-derived suffix makes the ID verifiable from the signed body alone, and an immutable ID keeps the trust lists stable.

**Rejected**
A plain name was considered and rejected because it cannot be authenticated.
A random suffix bound to the key only inside the body was considered and rejected because the ID itself would then prove nothing.

#### A FileRef is signed by inclusion in the Profile

**Decision**
A FileRef carries a name and a root hash and no signature; the Profile's signature covers the list.

**Rationale**
Signing each file adds a signature to every share and makes the author sign again for every file, while the Profile already states the complete set the author claims.
The Profile is the unit a trust decision applies to, so the signature belongs there.

**Rejected**
A per-file signed record was considered and rejected because it duplicates the Profile's statement and makes the file reference heavier.

#### The trust and block lists name Profile IDs

**Decision**
Both lists carry Profile IDs, and an entry appears in at most one list.

**Rationale**
Trust applies to an identity, not to a node or an address, and the Profile ID is the identity the signature protects.
A single entry shape keeps the evaluation and the wire simple.

#### Messages are inline up to 64 KiB and otherwise FileRefs

**Decision**
A message body is inline up to 64 KiB, and a larger body or an attachment is a FileRef.

**Rationale**
A bulletin-board post is usually small, and a fetch per post would make reading a Profile slow.
The inline bound keeps one Profile inside one exchange frame and pushes only the genuinely large data to the file protocol.

#### One active Profile governs publication and trust

**Decision**
The owner selects one active Profile; it authors new publications and is the root of the trust evaluation.

**Rationale**
The trust result depends on which lists are the root, and two roots at once would produce two answers in one client.
The owner can switch, but the switch is explicit.

#### Advertisement is separate from authorship

**Decision**
A node advertises the committed files it stores and the root hashes of the subscriptions that provide blocks.
A FileRef decides who claims a file, not who may serve it.

**Rationale**
A publisher that holds a committed file and a relay that holds its blocks are both useful providers, and FileExchanger already advertises both ([file-transfer.md §5.1](./file-transfer.md#51-participating-nodes-and-advertisement) and [§6.1](./file-transfer.md#subscriptions-are-advertised-as-locations-from-the-start)).
The trust display attributes a file through FileRefs, so a relay's advertisement does not make it an author.

**Rejected**
Advertising only the root hashes named by local Profiles was considered and rejected because a node that already holds the blocks of a subscription would become invisible until it re-shared the file.

### 8.2 Deferred

#### Profile key rotation

**Current state**
A Profile key cannot be replaced without creating a new Profile ID, and the new Profile is unknown to everyone who trusted the old one.

**Why not decided yet**
A succession statement that carries trust from the old key to the new one needs its own distribution and a rule for trusting the statement itself, and no deployment needs it yet.

**Decision condition**
Decide before any operation that must preserve authorship across a key replacement, such as a compromised key or a forced migration.

#### Renaming a Profile

**Current state**
The name is part of the ID, so renaming creates a new identity.

**Why not decided yet**
A rename that keeps the ID and the trust would make the ID's display part mutable and weaken the one-to-one binding between the ID and its statement.

**Decision condition**
Decide when a user needs to change the display name of an existing Profile.

#### Profile metadata beyond the lists

**Current state**
The body carries only the identity, the lists, and the messages.

**Why not decided yet**
An avatar, a description, or a display color has no consumer yet and would grow every snapshot.

**Decision condition**
Decide before the client shows a field that the body does not carry.

#### Deleting a local Profile

**Current state**
A local Profile can be created and activated, but not deleted.

**Why not decided yet**
Deleting a local Profile removes its key and body, but the Profile ID may still be referenced by trust lists and by copies that other nodes serve. Whether to keep a tombstone or to forbid deletion while references remain is not decided.

**Decision condition**
Decide when the owner needs to remove a local Profile.

## 9. Current status and remaining work

The design is not implemented.
Only the FileRef type exists, and the engine has an empty placeholder for the Profile exchange.
The client has no Profile screen.

The next work is the implementation of this document and [profile-exchanger.md](./profile-exchanger.md#1-about-this-document), followed by [web-of-trust.md](./web-of-trust.md#1-about-this-document), in the order of [design.md §6.2](../design.md#62-roadmap).

For confirmed bugs, see [issues.md](../issues.md).
