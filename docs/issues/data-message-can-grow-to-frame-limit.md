# 最大構成の DataMessage が frame 上限の約 4 MiB になり得る

**深刻度: 低**（NodeFinder の接続先が既知 node に限られるため）

調査日 2026-10-07 / 対象コミット `9bbbcbd`

[issues.md](../issues.md) の 1 項目。

## 症状

NodeFinder の DataMessage は、各配列の件数上限を最大に詰めると frame 上限の 4 MiB 近くになる。
受信側は frame 全体を buffer してから decode するため、4 MiB 級の割り当てが接続の数だけ同時に生じ得る。

## 該当箇所

[framed.rs:16](../../daemon/modules/engine/src/base/connection/stream/framed.rs#L16) の `NODE_FINDER_MAX_FRAME_LENGTH`（4 MiB）が、DataMessage の大きさに対する唯一の上限である。

```rust
pub const NODE_FINDER_MAX_FRAME_LENGTH: usize = 4 * 1024 * 1024;
```

[data_message.rs:17](../../daemon/modules/engine/src/core/negotiator/node/data_message.rs#L17) の各上限は件数だけである（`push_node_profiles` 32、`want_asset_keys` 1024、`give_asset_key_locations` と `push_asset_key_locations` 1024、location 内の node profiles 8）。
[node_profile.rs:12](../../daemon/modules/engine/src/model/node_profile.rs#L12) の NodeProfile は、`public_key` と addr の文字列に長さの上限を持たない。

## 原因

DataMessage の要素ごとの上限が件数だけで、NodeProfile の wire 表現の大きさ（`public_key` の長さ、addr の文字列長）が未定義である。
そのため frame 上限が実効上限になり、最大構成の message が上限いっぱいまで大きくなり得る。

## 影響

1 frame につき最大 4 MiB の buffer と、decode 後の中間表現がメモリに乗る。
接続数が増えると割り当てが同時多発する。
コードを読んで確認した。最大構成での encode サイズの実測は行っていない。

## 対応方針

NodeProfile の `public_key` と addr の文字列に wire の長さ上限を設ける。
DataMessage の実効上限を frame 上限より小さく抑え、最大構成で encode したときの大きさを test で固定する。
