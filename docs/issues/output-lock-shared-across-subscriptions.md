# output_lock が全購読で共通で、出力中の購読が他の購読の確定を止める

**深刻度: 中**（出力の長い購読があるときに他の購読の確定が遅れる）

調査日 2026-10-07 / 対象コミット `9bbbcbd`

[issues.md](../issues.md) の 1 項目。

## 症状

1 つの購読の一時出力の書き出し中、他のすべての購読の出力操作（finalize、discard_output、sweep）が lock の解放を待つ。
大きな file の購読が出力中だと、完了した他の購読の出力確定が長く止まる。

## 該当箇所

[store.rs:26](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L26) のとおり、`FileSubscriberStore` は `state_dir` あたり 1 つで、`output_lock` も 1 つだけある。

```rust
output_lock: Arc<TokioMutex<()>>,
```

[store.rs:202](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L202) の `create_output` は lock の guard を `OutputWriter`（[同ファイル:469](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L469)）に渡し、出力の書き出しが終わるまで保持する。
[store.rs:257](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L257) の `finalize`、[同ファイル:356](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L356) の `discard_output`、[同ファイル:431](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L431) の `sweep_outputs_locked` が同じ lock を待つ。
[file_subscriber.rs:31](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber.rs#L31) のとおり、`FileSubscriber` が持つ store は 1 つで、すべての購読が共有する。

## 原因

購読間の出力先の衝突（同じ出力先の保護）を、購読全体で 1 つの lock で守っている。
lock の粒度が購読（行）より粗い。

## 影響

出力が止まることはないが、購読の完了から確定までの時間が、他の購読の出力時間に引っ張られる。
出力の長い購読と短い購読が混在すると、短い購読の確定が長く遅れる。
コードを読んで確認した。実行による再現は行っていない。

## 対応方針

lock を購読（出力先）ごとの粒度に替える。
`OutputWriter` が持つ guard を出力先ごとの lock にし、`finalize` と `discard_output` は当該の購読の lock だけを取る。
1 つの購読が出力中でも、別の購読の `finalize` が完了することを test する。
