# SessionConnector が用途選択の結果 Unknown を受理する

**深刻度: 低**（受理側が `Unknown` を返す場合に限る。現在の SessionAccepter は `Accept` と `Reject` だけを返す）

調査日 2026-10-11 / 対象コミット `794035e`

[issues.md](../issues.md) の 1 項目。

## 症状

接続側は、用途の要求に対して受理側が `V1ResultType::Unknown` を返すと、Session が受理されたものとして上位へ渡す。

## 該当箇所

[connector.rs:94](../../daemon/modules/engine/src/core/session/connector.rs#L94) は、結果が `Reject` の場合だけエラーを返す。

```rust
if received_session_result_message.result_type == V1ResultType::Reject {
```

[message.rs:28](../../daemon/modules/engine/src/core/session/message.rs#L28) の `V1ResultType` は `Unknown`、`Accept`、`Reject` の 3 値を持つ。

## 原因

受理の条件を「`Accept` である」ではなく「`Reject` でない」として書いている。

## 影響

受理側が `Unknown` を返した Session を、接続側だけが確立済みとして扱う。
[accepter.rs:277](../../daemon/modules/engine/src/core/session/accepter.rs#L277) の SessionAccepter は、受理しない要求に `Reject` を返すため、Axus の node どうしでは起きない。
`Unknown` を返すのは、別の実装か、意図して値を変えた相手に限られる。
相手は secure channel で認証済みである。
NodeFinder の Session は、相手が応答しなければ [task_communicator.rs:177](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L177) の受信期限で閉じる。
FileExchanger の Session には確立後の送受信の loop と期限がなく（[session-accept-and-message-without-deadline](./session-accept-and-message-without-deadline.md)）、[task_connector.rs:214](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L214) で channel に渡されたまま残る。

## 対応方針

結果が `Accept` の場合だけ Session を返し、それ以外はエラーにする。
受理側が `Unknown` を返したときに接続がエラーになることを test で守る。
