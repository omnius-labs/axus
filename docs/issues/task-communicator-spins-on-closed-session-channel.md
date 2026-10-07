# TaskCommunicator::start が受理 channel が閉じても loop を抜けない

**深刻度: 低**（受理 channel の sender が drop されたときに限る）

調査日 2026-10-07 / 対象コミット `9bbbcbd`

[issues.md](../issues.md) の 1 項目。

## 症状

NodeFinder の Session 受理 channel が閉じると、`recv` は即座に `None` を返し続ける。
`TaskCommunicator::start` の loop は完了済み task の整理と受信を繰り返す busy loop になり、cancellation で止めない限り CPU を使い続ける。

## 該当箇所

[task_communicator.rs:87](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L87) の loop の中で、[同ファイル:91](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L91) の `select!` は `recv()` が `None` を返しても break せず、[同ファイル:95](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L95) の `if let Some(status)` を読み飛ばして loop の先頭に戻る。

```rust
let status = select! {
    status = async { this.session_receiver.lock().await.recv().await } => status,
    _ = this.cancellation_token.cancelled() => break,
};
if let Some(status) = status {
```

## 原因

`recv()` の `None`（channel 閉鎖）を「受信がなかった」と同じ扱いにしている。
channel 閉鎖は恒久的な状態であり、受信の不在とは違って break する必要がある。

## 影響

現状の構成では、NodeFinder が生きている間は sender も生き続けるため顕在化しない。
シャットダウン順序の変更や sender の drop が起きると、終了まで busy loop が続く。
コードを読んで確認した。実行による再現は行っていない。

## 対応方針

`recv()` が `None` を返したら loop を break する（channel 閉鎖は回復しない）。
受理 channel が閉じた状態で start の task が終了することを test する。
