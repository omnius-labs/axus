# Kadex::find が 2 件以上を求めると近い node を取りこぼす

**深刻度: 低**（呼び出し元が 1 件しか求めないため現時点では未顕在）

調査日 2026-09-25 / 対象コミット `59054a2`

[issues.md](../issues.md) の 1 項目。

## 症状

`count` に 2 以上を渡し、候補が距離の近い順に並んでいないとき、結果から近い node が欠ける。
3 件を求めて 3 つの候補を遠い順に渡すと、2 件しか返らないことを一時的な test で確かめた。

## 該当箇所

[kadx.rs:42](../../daemon/modules/engine/src/core/negotiator/node/kadx.rs#L42) の挿入位置からの繰り下げが、末尾の 1 つ手前で止まる。

```rust
for j in ((left + 1)..(results.len() - 1)).rev() {
```

## 原因

繰り下げの範囲が `results.len() - 1` で終わるため、末尾の直前の要素が末尾へ移らずに上書きされる。

## 影響

[task_computer.rs:190](../../daemon/modules/engine/src/core/negotiator/node/task_computer.rs#L190) と [task_computer.rs:208](../../daemon/modules/engine/src/core/negotiator/node/task_computer.rs#L208) は `count` に 1 を渡すため、いまの伝播には影響しない。
[NodeFinder の能動探索と冗長度](../design/node-finder.md#nodefinder-の能動探索と冗長度) を決めて複数の node へ送るようにすると顕在化する。
既存の test は候補を近い順に渡すため検出できない。

未使用の `Kadex::distance`（[kadx.rs:61](../../daemon/modules/engine/src/core/negotiator/node/kadx.rs#L61)）は先頭の byte を上位として扱い、`Kadex::compare`（[kadx.rs:78](../../daemon/modules/engine/src/core/negotiator/node/kadx.rs#L78)）は末尾の byte を上位として扱う。

## 対応方針

繰り下げの範囲を末尾まで広げる。
候補を遠い順と無作為な順で渡し、近い順の `count` 件が返ることを test する。
`distance` は `compare` と同じ byte order に揃えるか、使わないなら削除する。
