# FileExchanger の起動失敗が握り潰される

**深刻度: 低**（FileExchanger が未結線のため現時点では未顕在）

調査日 2026-09-25 / 対象コミット `59054a2`

[issues.md](../issues.md) の 1 項目。

## 症状

FilePublisher または FileSubscriber の初期化に失敗しても、`FileExchanger::new` は成功を返す。

## 該当箇所

[file_exchanger.rs:93](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L93) は `start` の `Result` を捨てる。

```rust
v.start().await;
```

[file_exchanger.rs:57](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L57) の `#[allow(unused)]` が `unused_must_use` も抑えるため、lint でも検出されない。

## 原因

`start` の失敗を呼び出し元へ伝えていない。

## 影響

storage を開けないときも FileExchanger が生成され、publisher と subscriber のない状態で接続 task だけが動く。
AxusService へ結線すると顕在化する。

## 対応方針

`start` の結果を `?` で返す。
`#[allow(unused)]` を関数全体ではなく、未使用の引数や field に限って付ける。
state directory を作れない path で `FileExchanger::new` が失敗することを test する。
