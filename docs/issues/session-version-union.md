# version 交渉が積集合ではなく和集合になっている

**深刻度: 中**（V1 しか存在しないため現時点では未顕在）

調査日 2026-09-02 / 対象コミット `8970f441df9629246e5c6b536334683f79f3953a`

[issues.md](../issues.md) の 1 項目。

## 症状

対応 version が複数になった場合、双方に共通しない version でも交渉が成立する。
その結果、互いに解釈できない message を交換する。

## 該当箇所

次の 3 箇所が bit flag の和集合を取っている。

- [connector.rs:37](../../daemon/modules/engine/src/core/session/connector.rs#L37)
- [accepter.rs:174](../../daemon/modules/engine/src/core/session/accepter.rs#L174)
- [task_communicator.rs:149](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L149)

```rust
let version = send_hello_message.version | received_hello_message.version;
```

## 原因

双方が対応する version の積集合ではなく、どちらかが対応する version の和集合を計算している。

## 影響

V1 だけが定義されている間は結果が変わらない。
V2 以降を追加すると、接続成立後に message の解釈が食い違う。

## 対応方針

3 箇所の `|` を `&` に変更する。
積集合が空の場合は、対応 version がないことを示す error を返す。
複数の共通 version から 1 つを選ぶ規則は [session.md](../design/session.md#5-設計判断) で決める。
共通 version がない場合と複数ある場合の test を追加する。
