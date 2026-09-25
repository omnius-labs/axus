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
| FileSubscriber | file-transfer | `FileSubscriber` | - | root hash から block を集めて file を復号する component | - |
| MerkleLayer | file-transfer | `MerkleLayer` | - | 下位 block の hash 列と、その block の rank を格納する block の内容 | - |
| rank | file-transfer | `MerkleLayer.rank` | - | Merkle 構造における block の階層。file 本体の block を 0 とする | - |
| root hash | file-transfer | `OmniHash` | - | 最上位 block の hash である内容識別子 | - |
| AssetKey | node-finder | `AssetKey` | - | NodeFinder が所在を探索する対象の識別子 | ファイル参照 |
| NodeFinder | node-finder | `NodeFinder` | - | 既知 node を交換し、AssetKey の所在情報を探す component | - |
| 既知 node | node-finder | `NodeFinderRepo` | - | 接続の候補として保存した NodeProfile の集合 | - |
| NodeProfile | node-finder | `NodeProfile` | `axus:node/` URI | P2P network 上の node の公開鍵と到達先の組 | - |
| 到達先 | node-finder | `NodeProfile.addrs` | - | NodeProfile に載せる、その node へ接続できるアドレスの列 | - |
| node ID | node-finder | `NodeProfile::id()` | - | node の公開鍵の SHA3-256 hash。Kadex の距離計算と Session の重複判定に使う | NodeProfile.id |
| bootstrap node | node-finder | `bootstrap_node_profiles` | `p2p.bootstrap_nodes` | 起動時に既知 node へ加える、設定で与えた NodeProfile | - |
| 所在情報 | node-finder | `give_asset_key_locations` | - | AssetKey と、それを提供する NodeProfile 群の対応 | - |
| FileRef | profile-memo | `FileRef` | - | 上位データが公開済み file を参照する名前と hash の組 | ファイル本体 |
| memo | profile-memo | `MemoExchanger` | - | Profile を探索、交換、配布する下位機構 | 投稿本文 |
| Profile | profile-memo | `Profile` | - | 主体の公開情報と公開データへの FileRef 群を表す上位データ | - |
| rpf | rocketpack | `.rpf` | `.rpf` file | RocketPack 型と field 番号を定義する schema file | Rust 型定義 |
| Session | session | `Session` | - | 相手の証明、接続方向、用途を伴う通信路 | TCP 接続 |

## 3. 隣接語の境界

<a id="t-node-profile-vs-session"></a>
#### NodeProfile と Session

**境界**
NodeProfile は探索に使う node の公開鍵と到達先である。
Session は challenge への署名を確認した後に用途へ渡す通信路であり、署名者がその公開鍵の秘密鍵を持つことだけを保証する。
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
