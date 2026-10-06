# delete_bulk が存在しない key を含むと何も削除せずに成功する

**深刻度: 低**（現在の呼び出しは存在する key だけを渡すため未顕在）

調査日 2026-10-06 / 対象コミット `8432b83`

[issues.md](../issues.md) の 1 項目。

## 症状

`KeyValueRocksdbStorage::delete_bulk` に存在しない key が 1 つでも含まれると、その key より前に積んだ削除も含めて何も書かずに成功を返す。
削除されなかった key は、呼び出し元から見て削除済みに見える。

## 該当箇所

[key_value_rocksdb_storage.rs:430](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L430) は、名前から内部 ID を引けなかった key で `return Ok(())` し、[同ファイル:438](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs#L438) の `write_opt` に届かない。

```rust
None => return Ok(()),
```

## 原因

存在しない key を読み飛ばすには `continue` が必要だが、`return Ok(())` と書かれており、batch の書き込みに届かない。
`test_delete_bulk` は、存在しない key だけを渡しても error にならないことと、存在する key だけを渡して削除されることを別々に確かめ、両者が混在する場合を確かめていない。

## 影響

唯一の呼び出し元の `delete_prefix`（[store.rs:189](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L189)）は、同じ mutex 内で列挙した key だけを渡すため、現時点では影響しない。
存在する key と存在しない key が混在する呼び出しを足すと、削除されない key が残る。
コードを読んで確認した。実行による再現は行っていない。

## 対応方針

存在しない key は `continue` で読み飛ばし、残りを 1 つの batch で削除する。
存在する key と存在しない key が混在する呼び出しで、存在する key がすべて削除されることを test する。
