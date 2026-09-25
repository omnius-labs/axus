# 応答しない接続が Session の受理と発信を止める

**深刻度: 高**（外部から誘発可能）

調査日 2026-09-25 / 対象コミット `59054a2`

[issues.md](../issues.md) の 1 項目。

## 症状

何も送らない TCP 接続を 3 本開いたままにすると、その node はほかの Session を一切受理しなくなる。
ログには何も出ず、受理側の NodeFinder の Session が増えなくなるだけである。
発信側でも、TCP の接続を受け付けたまま応答しない相手へ接続した TaskConnector は戻らない。

## 該当箇所

[accepter.rs:76](../../daemon/modules/engine/src/core/session/accepter.rs#L76) は受理 worker を 3 つだけ起動する。
各 worker は [accepter.rs:168](../../daemon/modules/engine/src/core/session/accepter.rs#L168) で TCP 接続を 1 本受け取り、[accepter.rs:172](../../daemon/modules/engine/src/core/session/accepter.rs#L172) から [accepter.rs:191](../../daemon/modules/engine/src/core/session/accepter.rs#L191) までの `recv_message` を期限なしで待ってから次の接続へ進む。

発信側の [connector.rs:35](../../daemon/modules/engine/src/core/session/connector.rs#L35) から [connector.rs:61](../../daemon/modules/engine/src/core/session/connector.rs#L61) も期限を持たない。
NodeFinder の 3 つの TaskConnector（[node_finder.rs:150](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L150)）は、[task_connector.rs:158](../../daemon/modules/engine/src/core/negotiator/node/task_connector.rs#L158) でこの `connect` を待つ。

確立後の Session も、[task_communicator.rs:275](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L275) の受信を期限なしで待つ。

## 原因

handshake の各段と確立後の受信に期限がなく、受理 worker が 1 本の接続の handshake を終えるまで次の接続を受け取らない。

## 影響

認証前の相手が 3 本の接続だけで、その node への NodeFinder の新しい Session を止められる。
接続先の選択に使う既知 node には、他の node から受け取った NodeProfile が検証なしに入る（[task_communicator.rs:277](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L277)）。
そのため、応答しない到達先を載せた NodeProfile を配ると、発信側の TaskConnector も順に止められる。
確立後に黙った相手は、受理側の Session の上限（3 本）を占有し続ける。

in-memory の接続で 3 本の無応答接続を開いたまま `SessionConnector::connect` を呼ぶと、3 秒待っても完了しないことを一時的な test で確かめた。

## 対応方針

handshake 全体と確立後の受信に期限を設け、期限を過ぎた接続を閉じる。
受理 worker は TCP の受理と handshake を分け、handshake を接続ごとの task で進めて、同時に進める handshake の数に上限を置く。
無応答の接続を上限まで開いた後でも、正しく応答する相手の Session が確立することを test する。
