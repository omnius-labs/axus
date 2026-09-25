# 永続化の設計

## 1. このドキュメントについて

本書は node と file の metadata、block 実体、状態 directory、schema migration の設計を扱う。
語の定義は [terms.md](../terms.md) が正とする。

### 1.1 文書間の責務分担

| 文書または正本 | 受け持つもの |
| --- | --- |
| [design.md](../design.md#11-文書間の責務分担) | 全体構成、他の関心事との境界、横断的な依存関係 |
| 本書 | metadata と block 実体の保存、状態 directory、migration |
| [file-transfer.md](./file-transfer.md#2-責務と境界) | 永続化する file の状態遷移と block の検証 |
| [base/storage](../../daemon/modules/engine/src/base/storage) | block storage の実装 |
| [core/negotiator](../../daemon/modules/engine/src/core/negotiator) | SQLite repository と migration の実装 |

### 1.2 本書の時制について

本文は完成形の設計を記述する。
実装状況と残作業は §6 に集約する。

## 2. 責務と境界

永続化層は、状態遷移と条件検索を必要とする metadata と、hash key で読み書きする block 実体を別の storage model で扱う。
file の protocol、探索規則、REST API の contract は持たない。

## 3. metadata と block の分離

| 種別 | 実体 | 保存するもの |
| --- | --- | --- |
| metadata | SQLite と `omnius-core-migration` | 既知 node、file 状態、block index |
| block | `KeyValueRocksdbStorage` | file 本体と MerkleLayer の byte 列 |

metadata は条件検索と状態遷移を必要とするため SQLite に置く。
block は hash key による読み書きが中心であり、大きな byte 列を metadata query から分離するため RocksDB に置く。

### 3.1 `state_dir` の構造

永続状態は component ごとに directory を分ける。

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

publisher と subscriber の保存領域を分けることで、一方の再構築や削除が他方の状態を直接壊さない。
`identity/` は node の署名鍵を持ち、消すと次の起動で別の鍵と node ID が作られるため、ほかの directory と違って作り直しでは元に戻らない。
directory 名の追加や移行が必要な場合は migration と rollback の単位を明示する。

## 4. 論理 key と migration

[KeyValueRocksdbStorage](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs) は、論理 key から内部 ID を引く `names` と、内部 ID から実体を引く `blocks` および `metas` を分ける。
公開処理の commit は論理 key を uncommitted から committed へ変更し、同じ block 実体を再利用する。

**rename の原子性は、対象が同じ RocksDB transaction 内にあることを前提とする。**
別 database または別 filesystem へまたがる移動を追加した場合、この前提は崩れ、copy と recovery protocol が必要になる。

[publisher_repo.rs](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs)、[subscriber_repo.rs](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs)、[node_finder_repo.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs) は、名前付きの MigrationRequest を順に適用する。
適用済みの名前を持つ migration は再実行しない。

配布済みの状態へ適用され得る migration は書き換えず、新しい名前の migration で前進させる。
初回 release 前の migration を直接修正する場合でも、適用済み状態が存在しないことを確認し、その確認結果を変更記録に残す。

## 5. 設計判断

### 5.1 決定済み

#### metadata と block 実体を別の storage に置く

**決定**
検索と状態遷移を伴う metadata は SQLite に置き、hash key で扱う block 実体は RocksDB に置く。

**理由**
関係 query と大きな byte 列の読み書きを同じ storage model に押し込めず、それぞれの access pattern に合わせるためである。

### 5.2 保留

この関心事に閉じる保留はない。

## 6. 現状と残作業

NodeFinder、publisher、subscriber の repository と block storage がある。
publisher と subscriber の repository は、すべての public method を test で実行している。

確認済みの不具合は [issues.md](../issues.md) を参照する。
