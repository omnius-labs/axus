# Axus 用語集

## 1. このドキュメントについて

本書は Axus 固有の語彙の唯一の正本である。
語の定義、表記、別称、隣接語との境界を扱う。

### 1.1 文書間の責務分担

| 文書 | 受け持つもの |
| --- | --- |
| 本書 | 語の定義、表記、別称、隣接語との境界、命名の理由 |
| [design.md](./design.md#11-文書間の責務分担) | 設計の理由、不変条件、実装状況、保留 |
| [issues.md](./issues.md) | コードで確認した不具合と対応方針 |

文書族の地図は [design.md](./design.md#11-文書間の責務分担) が持つ。

## 2. 用語一覧

| 用語 | スコープ | 識別子 | 外部表現 | 指すもの | 使わない別称 |
| --- | --- | --- | --- | --- | --- |
| FileExchanger | file-transfer | `FileExchanger` | - | NodeFinder の探索結果を使って相手と Session を張り、block を運ぶ component | - |
| FilePublisher | file-transfer | `FilePublisher` | - | local file を block と MerkleLayer に変換して公開する component | - |
| 公開 commit | file-transfer | `FilePublisherStore::commit` | - | uncommitted file の root hash と保存済み block を committed file として確定する操作 | - |
| Store | file-transfer | `FilePublisherStore`、`FileSubscriberStore` | - | metadata と block 実体を変更し、出力 file の作成・確定・削除と永続化の整合性を管理する component | - |
| FileSubscriber | file-transfer | `FileSubscriber` | - | root hash から block を集めて file を復号する component | - |
| 出力確定 | file-transfer | `Finalizing` → `Completed` | - | 復号済み一時出力を置換しない rename で利用者の出力先へ配置・永続化し、Completed を記録する操作 | - |
| MerkleLayer | file-transfer | `MerkleLayer` | - | 下位 block の hash 列と、その block の rank を格納する block の内容 | - |
| rank | file-transfer | `MerkleLayer.rank` | - | Merkle 構造における block の階層。file 本体の block を 0 とする | - |
| root hash | file-transfer | `OmniHash` | - | 最上位 block の hash である内容識別子 | - |
| 出力先予約 | file-transfer | - | - | 進行中の購読の出力先を、ほかの購読が出力先にできないようにする購読の行の一意制約 | - |
| committed file | file-transfer | `PublishedCommittedFile` | - | root hash を確定し、network へ広告できる公開側の file。root hash と file 名の組で識別する | - |
| uncommitted file | file-transfer | `PublishedUncommittedFile` | - | import してから root hash を確定するまでの公開側の file。一時 ID で識別する | - |
| 一時出力 | file-transfer | - | `.<entry 名>.axus-<購読 ID>.part` | 購読ごとに出力先と同じ directory へ規約の名前で作り、復号結果を書き込む file | - |
| Finalizing | file-transfer | `SubscribedFileStatus::Finalizing` | - | 一時出力を永続化して確定を開始し、Completed または Failed へ至るまでの購読状態 | - |
| Task | file-transfer | `TaskDecoder`、`TaskEncoder` | - | 符号化・復号、確定前の取り消し、sweep と出力配置の再試行を制御する component。FileExchanger の接続用 task とは別である | - |
| AssetKey | node-finder | `AssetKey` | - | NodeFinder が所在を探索する対象の識別子 | ファイル参照 |
| bootstrap node | node-finder | `bootstrap_node_profiles` | `p2p.bootstrap_nodes` | 起動時に既知 node へ加える、設定で与えた NodeProfile | - |
| 所在情報 | node-finder | `give_asset_key_locations` | - | AssetKey と、それを提供する NodeProfile 群の対応 | - |
| NodeFinder | node-finder | `NodeFinder` | - | 既知 node を交換し、AssetKey の所在情報を探す component | - |
| 既知 node | node-finder | `NodeFinderRepo` | - | 接続の候補として保存した NodeProfile の集合 | - |
| NodeProfile | node-finder | `NodeProfile` | `axus:node/` URI | P2P network 上の node の公開鍵と到達先の組 | - |
| 到達先 | node-finder | `NodeProfile.addrs` | - | NodeProfile に載せる、その node へ接続できるアドレスの列 | - |
| node ID | node-finder | `NodeProfile::id()` | - | node の公開鍵の SHA3-256 hash。Kadex の距離計算と Session の重複判定に使う | NodeProfile.id |
| FileRef | profile-memo | `FileRef` | - | 上位データが公開済み file を参照する名前と hash の組 | ファイル本体 |
| memo | profile-memo | `MemoExchanger` | - | Profile を探索、交換、配布する下位機構 | 投稿本文 |
| Profile | profile-memo | `Profile` | - | 主体の公開情報と公開データへの FileRef 群を表す上位データ | - |
| rpf | rocketpack | `.rpf` | `.rpf` file | RocketPack 型と field 番号を定義する schema file | Rust 型定義 |
| Session | session | `Session` | - | 相手の証明、接続方向、用途を伴う通信路 | TCP 接続 |
| sweep | storage | - | - | RocksDB の key と一時出力を SQLite の行と照合し、参照のないものを消す回収処理 | - |

## 3. 隣接語の境界

<a id="t-publisher-commit-vs-output-finalization"></a>
#### 公開 commit と出力確定

**境界**
公開 commit は、block と root hash を広告可能な committed file にする。
出力確定は、購読の復号結果を filesystem の出力先へ配置・永続化し、その後に Completed を記録する。
DB の commit は、それぞれの処理を構成する手順の 1 つである。

**取り違えると何が起きるか**
出力確定を Completed の DB commit だけと扱うと、配置と directory の永続化が完了する前に成功を返し、障害後に出力を失い得る。

<a id="t-root-hash-vs-output-reservation"></a>
#### root hash と出力先予約

**境界**
root hash は内容を識別し、購読間の block 共有に使う。
出力先予約は filesystem の同じ entry への競合を防ぐ。

**取り違えると何が起きるか**
root hash を予約の単位にすると、同じ内容を別の出力先へ購読できなくなり、異なる内容を同じ出力先へ購読する競合も防げない。

<a id="t-node-profile-vs-session"></a>
#### NodeProfile と Session

**境界**
NodeProfile は探索に使う node の公開鍵と到達先である。
Session は相手の署名を検証して鍵を合意した後に用途へ渡す通信路であり、相手が誰であるかについては、通信の相手がその公開鍵の秘密鍵を持つことだけを保証する。
両者の公開鍵の照合は [trust-security.md](./design/trust-security.md#node-id-は公開鍵から導出する) が扱う。

**取り違えると何が起きるか**
NodeProfile を受け取っただけで、その到達先や相手への信頼まで確定したものとして扱うと、署名のない到達先と未決の trust の contract を迂回する。

<a id="t-asset-key-vs-file-ref"></a>
#### AssetKey と FileRef

**境界**
AssetKey は network 上で所在を探す key である。
FileRef は表示名と内容を参照する hash を上位データへ渡す値であり、探索結果を含まない。

**取り違えると何が起きるか**
FileRef を探索 key として扱うと、表示名を含む上位表現へ NodeFinder の探索 contract が漏れる。

<a id="t-file-ref-vs-root-hash"></a>
#### FileRef と root hash

**境界**
root hash は内容だけを識別する hash である。
FileRef は上位データが内容へ名前付きで参照する値である。

**取り違えると何が起きるか**
root hash だけを FileRef として扱うと、上位 service が保持すべき表示名と分類を失う。

<a id="t-profile-vs-memo"></a>
#### Profile と memo

**境界**
Profile は公開情報と FileRef 群を表す payload である。
memo はその payload を探索、交換、配布する下位機構であり、投稿本文や添付の意味を解釈しない。

**取り違えると何が起きるか**
memo が Profile の内容を解釈すると、上位データ model の変更が探索と配布の protocol へ波及する。
