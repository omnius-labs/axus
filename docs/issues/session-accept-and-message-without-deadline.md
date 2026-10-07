# 確立後の send_message と受理待ちの Session に期限がない

**深刻度: 中**（期限を掛けない呼び出し側が増えたときに顕在化）

調査日 2026-10-07 / 対象コミット `9bbbcbd`

[issues.md](../issues.md) の 1 項目。

## 症状

handshake を完了した Session が、`SessionAccepter` の channel に積まれたまま期限なしで残る。
確立後の `send_message` と `recv_message` に期限の仕組みがなく、呼び出し側が timeout を掛けない場合、黙った相手からの受信を待つ task が永久に待ち続ける。

## 該当箇所

[accepter.rs:70](../../daemon/modules/engine/src/core/session/accepter.rs#L70) の `accept` は、channel からの受信を期限なしで待つ。

```rust
receiver.recv().await.ok_or_else(|| Error::new(ErrorKind::EndOfStream).with_message("receiver is closed"))
```

[packet.rs:9](../../daemon/modules/engine/src/base/connection/stream/packet.rs#L9) の `FramedRecvExt::recv_message` と、[同ファイル:26](../../daemon/modules/engine/src/base/connection/stream/packet.rs#L26) の `FramedSendExt::send_message` は、frame の長さ上限だけを守り、時間の概念を持たない。
[model.rs:24](../../daemon/modules/engine/src/core/session/model.rs#L24) の `SessionOption` は handshake の期限（`handshake_timeout`）だけを持ち、確立後の期限の項目がない。

## 原因

handshake の期限は `SessionOption` で一括管理するが、確立後の受信と受理待ちには同じ仕組みがない。
期限は各呼び出し側に任されており、NodeFinder の communicate が受信期限を掛けている（[session.md](../design/session.md) の設計判断）のは個別の対応であって、共通の仕組みではない。

## 影響

黙った相手との Session は、接続と受信 task を保持し続ける。
受理待ちの Session は channel 容量（20）まで積まれ、以降の handshake 完了 Session が詰まる。
将来の呼び出し側が期限を掛け忘れると、同じ問題を個別に再発させる。
コードを読んで確認した。実行による再現は行っていない。

## 対応方針

`SessionOption` に確立後の受信期限を追加し、Session または framed stream の recv/send に deadline を組み込む。
受理待ちにも期限を設けるか、未受理 Session の掃き出しを入れる。
NodeFinder の communicate が個別に掛けている受信期限を、共通の仕組みに置き換える。
