# 接続側が接続候補の公開鍵を Session の cert と照合しない

**深刻度: 中**（署名付き NodeProfile の到達先が別の node のものになった場合に顕在）

調査日 2026-10-12 / 対象コミット `4bb0571`

[issues.md](../issues.md) の 1 項目。

## 症状

NodeFinder と FileExchanger の接続側は、ある NodeProfile の到達先へ接続して Session を確立すると、相手の公開鍵を確かめずに、その NodeProfile の node へ接続したものとして記録する。
到達先が別の node のものであっても、その node が自分の鍵で Session を確立すれば、接続は成功として扱われる。

## 該当箇所

[node/task_connector.rs:157](../../daemon/modules/engine/src/core/negotiator/node/task_connector.rs#L157) は、接続候補の到達先へ接続し、成功すると [node/task_connector.rs:166](../../daemon/modules/engine/src/core/negotiator/node/task_connector.rs#L166) で候補の NodeProfile を接続済みに加える。

```rust
if let Ok(session) = self.session_connector.connect(addr, &SessionType::NodeFinder).await {
```

[file/task_connector.rs:214](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L214) と [file/task_connector.rs:222](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L222) も同じ形である。
どちらも、`session.cert` の公開鍵を接続候補の NodeProfile の公開鍵と比べない。

## 原因

Session は相手の署名と鍵を検証するが、接続側が期待する相手の公開鍵を受け取らない。
[node/task_communicator.rs:172](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L172) の照合は、相手が送った NodeProfile の公開鍵と、その相手自身の cert を比べるものであり、接続候補の公開鍵とは比べない。

## 影響

NodeProfile の署名により、第三者はある node の公開鍵に自分の到達先を付けて配布できない（[trust-security.md](../design/trust-security.md#nodeprofile-は-node-本人の署名で到達先を結ぶ)）。
ただし、署名付き NodeProfile の到達先は古くなり得る。
node がアドレスを変えた後、そのアドレスを別の node が使うと、接続側はその到達先で別の node と Session を確立し、元の node を接続済みとして接続候補から外す。
別の node は、本物の node の署名を作れないため、本物の node として認証されることはない。
NodeFinder の Session は、[node/task_communicator.rs:133](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L133) で相手が送った NodeProfile の node ID で登録されるため、第三者の node 自身の Session として扱われる。

## 対応方針

接続の成功後に、`session.cert` の公開鍵が接続候補の NodeProfile の公開鍵と一致することを確かめ、一致しなければ Session を閉じて次の到達先を試す。
公開鍵の異なる node が候補の到達先で応答したときに、候補を接続済みに加えないことを test で守る。
