# 受信 message の大きさが認証前から 64 MiB まで通る

**深刻度: 中**（外部から誘発可能）

調査日 2026-09-25 / 対象コミット `59054a2`

[issues.md](../issues.md) の 1 項目。

## 症状

Session の handshake 前の message を含むすべての受信 message が、1 frame 64 MiB まで受理される。
NodeFinder の DataMessage では、1 つの AssetKey に付ける NodeProfile の数に上限がない。

## 該当箇所

[framed.rs:9](../../daemon/modules/engine/src/base/connection/stream/framed.rs#L9) の上限は全 message に共通の 64 MiB である。

```rust
const MAX_FRAME_LENGTH: usize = 64 * 1024 * 1024;
```

DataMessage の decode は [task_communicator.rs:454](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L454) などで要素数を上限なしで読み、その数で `Vec` を確保する。
受信後に切り詰めるのは NodeProfile の数（[task_communicator.rs:277](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L277)）と、AssetKey の件数だけである。

## 原因

message の種類ごとの大きさと要素数の上限を定めず、frame の上限だけで入力を制限している。
core-rs の decoder は、残りの byte 数より多い要素数を拒否するため、確保量は frame の大きさに比例する範囲に留まる。

## 影響

認証前の相手が、受理 worker ごとに最大 64 MiB の frame を読ませられる。
認証後の相手は、1 つの AssetKey に多数の NodeProfile を付けた所在情報を送り、それを中継させられる。
[trust-security.md](../design/trust-security.md#3-保証の分担と脅威) が求める message size の上限は、この frame の上限でしか満たされていない。

## 対応方針

handshake 中の frame の上限を message に見合う大きさへ下げ、DataMessage の要素数と各所在情報の NodeProfile の数に上限を置く。
rpf へ移すときは、長さ制約付きの型で上限を表す（[rocketpack.md](../design/rocketpack.md#4-wire-format-の移行)）。
上限を超える message を受け取ったとき、Session を閉じることを test する。
