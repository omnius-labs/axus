# 自 node の NodeProfile が待ち受けアドレスを持たない

**深刻度: 高**（daemon に bootstrap の設定がなく、FileExchanger も構築されないため現時点では未顕在）

調査日 2026-09-24 / 対象コミット `2656848`

[issues.md](../issues.md) の 1 項目。

## 症状

NodeFinder が配布する自 node の NodeProfile の `addrs` が常に空である。
AssetKey の lookup で見つかった node にも、`push_node_profiles` で知った node にも接続できない。

## 該当箇所

[node_finder.rs:97](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L97) が、空の `addrs` で自 node の NodeProfile を作る。

```rust
my_node_profile: Arc::new(Mutex::new(NodeProfile {
    id: rng.clone().lock().random::<[u8; 32]>().to_vec(),
    addrs: Vec::new(),
})),
```

この NodeProfile は、[task_computer.rs:153](../../daemon/modules/engine/src/core/negotiator/node/task_computer.rs#L153) で `push_node_profiles` に入り、AssetKey を push する node として `give_asset_key_locations` にも入る。
[task_communicator.rs:110](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L110) は、同じ NodeProfile を handshake で相手へ送る。
`addrs` を設定する処理はほかにない。

## 原因

どの待ち受けアドレスを広告するかを決めないまま、NodeProfile の生成だけが実装されている。

## 影響

node は bootstrap に指定された NodeProfile にしか接続できず、複数 hop の探索が成立しない。
[file/task_connector.rs:200](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L200) は lookup で得た NodeProfile の `addrs` へ接続するため、FileExchanger を構築しても block を交換する相手に接続できない。

## 対応方針

[node-finder.md](../design/node-finder.md#自-node-の待ち受けアドレスの広告) で、広告するアドレスの決め方を決める。
アドレスを広告すると双方向の同時接続が起きるため、[重複した Session の解消規則](../design/node-finder.md#双方向の同時接続で重複した-session-の解消) も先に決める。
決めた規則で自 node の `addrs` を設定し、AssetKey の lookup が返す NodeProfile にアドレスが含まれることを結合試験で確認する。
