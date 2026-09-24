# `MerkleLayer.rank` の解釈が encoder と decoder で 1 ずれる

**深刻度: 高**（購読開始の入口がなく現時点では未顕在）

調査日 2026-09-02 / 対象コミット `8970f441df9629246e5c6b536334683f79f3953a`

[issues.md](../issues.md) の 1 項目。

## 症状

file の復号が最初の Merkle layer で `InvalidFormat` を返す。

## 該当箇所

encoder は [task_encoder.rs:228](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L228) で、格納先 block と同じ rank を記録する。

```rust
let mut rank = 1;
loop {
    let merkle_layer = MerkleLayer {
        rank,
        hashes: std::mem::take(&mut current_block_hashes),
    };
```

decoder は [task_decoder.rs:203](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L203) で、格納先 block より 1 小さい rank を期待する。

```rust
if merkle_layer.rank != (file.rank - 1) {
    return Err(Error::new(ErrorKind::InvalidFormat));
}
```

## 原因

`MerkleLayer.rank` が格納先 block と子 block のどちらの rank を表すかが確定していない。
encoder と decoder は異なる意味で実装されている。

## 影響

購読側の復号が成立しない。
購読開始の入口がないため、現在の daemon API からは顕在化しない。

## 対応方針

[file-transfer.md](../design/file-transfer.md#6-設計判断) で rank の意味を決める。
決定した意味に合わせて encoder と decoder を同時に修正する。
encode から decode までの round-trip test で contract を固定する。
