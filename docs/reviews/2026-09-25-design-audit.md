# 設計文書と daemon の監査レビュー

## 1. このレビューについて

設計文書群が daemon の実装と食い違っていないか、設計そのものに欠けている判断がないかを、実装を通しで読んで確かめた監査レビューである。

### 1.1 文書間の責務分担

| 文書 | 受け持つもの |
| --- | --- |
| 本書 | このレビューの範囲、観点、指摘、確かめて問題がなかったこと |
| [issues.md](../issues.md) | 種別 不具合と改善提案の転記先 |
| [terms.md](../terms.md) | 種別 用語の転記先 |
| [session.md](../design/session.md)、[trust-security.md](../design/trust-security.md)、[file-transfer.md](../design/file-transfer.md)、[daemon-api.md](../design/daemon-api.md)、[rocketpack.md](../design/rocketpack.md) | 種別 設計論点の転記先 |

文書族の地図は [design.md](../design.md#11-文書間の責務分担) が持つ。

### 1.2 レビュー日と対象

1. レビュー日: 2026-09-25
2. 型: 監査レビュー
3. 対象コミット: `59054a2`（core-rs は `5a92929`）
4. レビュアー: Claude Code（AI エージェント）

## 2. 範囲と観点

### 2.1 見た範囲

`docs/` の設計文書、用語集、不具合一覧のすべてと、`daemon/modules/engine/src` の Session、NodeFinder、FileExchanger、FilePublisher、FileSubscriber、block storage、`daemon/entrypoints/daemon/src` を読んだ。
設計文書が前提にする core-rs の RocketPack compiler の設計文書、DESIGN.md の RocketPack と secure connection の節、`OmniSigner` と `OmniSecureStream` の署名処理も読んだ。
範囲は、設計文書が責務を持つ関心事の実装で切った。

指摘 R-1、R-5、R-8 は、一時的な test を追加して再現を確かめ、その test は取り除いた。
engine の既存の test は 45 件すべてが通ることを確かめた。

### 2.2 見ていない範囲

- `daemon/entrypoints/interface` の生成物は、手で編集しない生成物であり範囲外とした
- core-rs の実装は、Axus から使う署名と decoder の箇所以外を範囲外とした
- `docs/coding/rust.md` への準拠は、lint が正本であり範囲外とした
- UPnP と SOCKS5 の経路は、実機の router と proxy がなく確かめる時間がなかった
- FileExchanger の `task_accepter.rs` と `session_status.rs` は、block 交換 protocol がなく処理の本体がないため読み流しに留めた
- 性能と資源の消費量は測っていない

### 2.3 観点

- 設計文書の記述が実装と一致しているか
- 外部から入力を受ける境界に、期限と大きさの上限があるか
- 認証が何を保証し、何を保証しないか
- 2 つの storage をまたぐ処理が、途中で終了しても回復できるか
- 設計文書が約束している流れに、実装上のデータの出所があるか

## 3. 総評

NodeFinder の探索と Session の重複解消は設計どおりに動いており、既知の不具合を除けば現状のまま使える。
一方、Session には接続確立の期限がなく（R-1）、認証が接続に束縛されていない（R-2）ため、信頼できない network へ出す前に直す必要がある。
公開処理は 2 つの storage をまたぐ commit の回復手順を欠いており（R-5）、FileExchanger を結線する前に直す必要がある。

## 4. 指摘

| ID | 概要 | 種別 | 確度 | 扱い |
| --- | --- | --- | --- | --- |
| [R-1](#r-1) | 応答しない接続が Session の受理と発信を止める | 不具合 | 高 | issues.md に起票 |
| [R-2](#r-2) | Session の認証が接続に束縛されていない | 設計論点 | 高 | session.md の保留へ転記 |
| [R-3](#r-3) | 到達先の真正性の保留が古い前提と誤った現状を持つ | 設計論点 | 高 | trust-security.md の保留へ転記 |
| [R-4](#r-4) | 受信 message の大きさを frame の上限でしか制限していない | 不具合 | 高 | issues.md に起票 |
| [R-5](#r-5) | commit の途中で終了した file を再び公開できない | 不具合 | 高 | issues.md に起票 |
| [R-6](#r-6) | block の hash を照合する位置が決まっていない | 設計論点 | 高 | file-transfer.md の保留へ転記 |
| [R-7](#r-7) | 公開側が接続先を探す手段がない | 設計論点 | 高 | file-transfer.md の保留へ転記 |
| [R-8](#r-8) | Kadex::find が 2 件以上で近い node を取りこぼす | 不具合 | 高 | issues.md に起票 |
| [R-9](#r-9) | REST API の到達範囲と認証が決まっていない | 設計論点 | 高 | daemon-api.md の保留へ転記 |
| [R-10](#r-10) | rocketpack.md が core-rs の古い compiler の仕様を複写している | 設計論点 | 高 | rocketpack.md の保留へ転記 |
| [R-11](#r-11) | 用語集の使わない別称に別の用語が入り、主要な語が欠けている | 用語 | 高 | terms.md の用語一覧へ転記 |
| [R-12](#r-12) | FileExchanger の起動失敗が握り潰される | 改善提案 | 高 | issues.md に起票 |
| [R-13](#r-13) | 多重起動の lock が state directory ではなく設定 directory にある | 改善提案 | 高 | issues.md に起票 |

R-1 と R-4 は、どちらも Session と NodeFinder の入力境界に期限と上限がないことから出ており、まとめて直すのが効率的である。
R-2 と R-3 は同じ原因から出ており、R-3 の判断は R-2 の判断に依存する。
R-6 と R-7 は、どちらも block 交換 protocol を定義する前に決める必要がある。

<a id="r-1"></a>
### R-1. 応答しない接続が Session の受理と発信を止める

**種別: 不具合** / **確度: 高**

**該当箇所**
[accepter.rs:76](../../daemon/modules/engine/src/core/session/accepter.rs#L76) が受理 worker を 3 つだけ起動し、各 worker は [accepter.rs:168](../../daemon/modules/engine/src/core/session/accepter.rs#L168) で受け取った接続の handshake を、[accepter.rs:172](../../daemon/modules/engine/src/core/session/accepter.rs#L172) 以降の期限のない受信で待つ。
発信側の [connector.rs:35](../../daemon/modules/engine/src/core/session/connector.rs#L35) も期限を持たない。

**何が問題か**
認証前の相手が何も送らない接続を 3 本開くだけで、その node は新しい Session を受理しなくなる。

**根拠**
in-memory の接続で無応答の接続を 3 本開いたまま `SessionConnector::connect` を呼び、3 秒で完了しないことを一時的な test で確かめた。
発信側が応答しない到達先で止まることは、コードを読んで確かめたが実行はしていない。

**影響**
NodeFinder の受理が止まり、既知 node に応答しない到達先を混ぜられると発信も止まる。

**扱い**
issues.md の session-handshake-without-deadline として起票した。

<a id="r-2"></a>
### R-2. Session の認証が接続に束縛されていない

**種別: 設計論点** / **確度: 高**

**該当箇所**
[accepter.rs:182](../../daemon/modules/engine/src/core/session/accepter.rs#L182) と [connector.rs:45](../../daemon/modules/engine/src/core/session/connector.rs#L45) は、相手から受け取った 32 byte の nonce だけに署名する。

```rust
let send_signature = self.signer.sign(&receive_challenge_message.nonce)?;
```

**何が問題か**
署名が TCP 接続にも handshake の内容にも束縛されていないため、第三者が handshake を中継すると、両端は互いを認証したまま第三者を経由して通信する。
また、認証前の相手がどの node にも任意の 32 byte への署名を作らせられる。

**根拠**
`OmniSigner::sign` は受け取った byte 列をそのまま Ed25519 で署名し、V1 の handshake には鍵合意も用途の接頭辞もないことをコードで確かめた。
core-rs の `OmniSecureStream` は自分の鍵合意の公開鍵を含む SHA3-256 の 32 byte に同じ種類の鍵で署名するため、同じ鍵で両方に応じると署名を流用できる。
中継となりすましは手順を追って確かめたが、実行はしていない。

**影響**
NodeProfile の到達先に署名がないため、偽の到達先を配れる第三者は、いまの daemon でも NodeFinder の通信の間に入れる。
secure channel の候補 1 を選んだ場合は、なりすましにつながる。

**扱い**
session.md の保留「Session の secure channel」へ転記した。

<a id="r-3"></a>
### R-3. 到達先の真正性の保留が古い前提と誤った現状を持つ

**種別: 設計論点** / **確度: 高**

**該当箇所**
trust-security.md の保留「NodeProfile の到達先の真正性」。
現状は偽の到達先ではなりすましが成立しないとし、今決めない理由は複数 hop の探索を daemon で動かしていないことだった。

**何が問題か**
R-2 の中継により、偽の到達先から通信の間に入れるため、現状の記述が誤っている。
複数 hop の探索は [service.rs:246](../../daemon/modules/engine/src/service.rs#L246) の 3 node の結合試験で動いており、今決めない理由が成り立たない。

**根拠**
R-2 と同じ。
結合試験が通ることは実行して確かめた。

**影響**
保留の決める条件が観測できない状態に留まり、判断の優先度を誤る。

**扱い**
trust-security.md の保留「NodeProfile の到達先の真正性」へ転記した。

<a id="r-4"></a>
### R-4. 受信 message の大きさを frame の上限でしか制限していない

**種別: 不具合** / **確度: 高**

**該当箇所**
[framed.rs:9](../../daemon/modules/engine/src/base/connection/stream/framed.rs#L9) の 64 MiB がすべての message に共通の上限であり、[task_communicator.rs:454](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L454) などは要素数を上限なしで読む。

**何が問題か**
認証前の handshake の message も 64 MiB まで読まれ、所在情報 1 件に付ける NodeProfile の数にも上限がない。

**根拠**
コードで確かめた。
core-rs の decoder は残りの byte 数を超える要素数を拒否するため、確保量は frame の大きさに比例する範囲に留まることも確かめた。
実際の memory 消費量は測っていない。

**影響**
trust-security.md の §3 が求める message size の上限が、frame の上限でしか満たされていない。

**扱い**
issues.md の unbounded-message-sizes として起票した。

<a id="r-5"></a>
### R-5. commit の途中で終了した file を再び公開できない

**種別: 不具合** / **確度: 高**

**該当箇所**
[task_encoder.rs:305](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L305) で block ごとに RocksDB の key を rename し、[task_encoder.rs:309](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L309) で SQLite に commit する。

**何が問題か**
その間で process が終了すると、再起動後の符号化が rename の `AlreadyExists` で失敗し、file は Failed のまま残る。

**根拠**
committed の key を先に置いた storage で同じ file を公開し、Failed になり理由が `AlreadyExists` であることを一時的な test で確かめた。
process の終了そのものは起こしていない。

**影響**
公開処理が途中で止まった file は利用者が消して入れ直すまで公開されず、参照されない block が残る。

**扱い**
issues.md の publisher-commit-not-crash-safe として起票した。

<a id="r-6"></a>
### R-6. block の hash を照合する位置が決まっていない

**種別: 設計論点** / **確度: 高**

**該当箇所**
[subscriber.rs:123](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber.rs#L123) の `write_block` は、[subscriber.rs:130](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber.rs#L130) で受け取った値を照合せずに保存する。

**何が問題か**
file-transfer.md は内容 hash の検証を commit より前に行うとするが、保存の入口で照合するか protocol 層で照合するかを決めていない。

**根拠**
コードで確かめた。

**影響**
protocol 層だけで照合すると、`write_block` を呼ぶ経路が増えるたびに照合を忘れる余地が生まれる。

**扱い**
file-transfer.md の保留「block の hash を照合する位置」へ転記した。

<a id="r-7"></a>
### R-7. 公開側が接続先を探す手段がない

**種別: 設計論点** / **確度: 高**

**該当箇所**
[task_connector.rs:196](../../daemon/modules/engine/src/core/negotiator/file/task_connector.rs#L196) は公開用の接続でも `find_node_profile` を呼び、[node_finder.rs:215](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L215) は提供する node の所在情報だけを返す。

**何が問題か**
file-transfer.md は公開側が要求する node へ接続するとしていたが、NodeFinder の want は要求元の NodeProfile を運ばないため、公開側は要求する node を知る手段を持たない。

**根拠**
DataMessage の want が `AssetKey` の列だけであることをコードで確かめた。

**影響**
公開用の接続 task は、同じ file を提供するほかの node へ接続する。

**扱い**
file-transfer.md の保留「公開側の接続先」へ転記した。

<a id="r-8"></a>
### R-8. Kadex::find が 2 件以上で近い node を取りこぼす

**種別: 不具合** / **確度: 高**

**該当箇所**
[kadx.rs:42](../../daemon/modules/engine/src/core/negotiator/node/kadx.rs#L42) の繰り下げが末尾の 1 つ手前で止まる。

**何が問題か**
候補が近い順に並んでいないと、3 件を求めても 2 件しか返らない。

**根拠**
3 つの候補を遠い順に渡して 3 件を求め、2 件しか返らないことを一時的な test で確かめた。

**影響**
いまの呼び出し元は 1 件しか求めないため顕在化していない。

**扱い**
issues.md の kadex-find-drops-candidates として起票した。

<a id="r-9"></a>
### R-9. REST API の到達範囲と認証が決まっていない

**種別: 設計論点** / **確度: 高**

**該当箇所**
[openapi.yaml:25](../../daemon/openapi.yaml#L25) の health は `security: []` であり、[server.rs:37](../../daemon/entrypoints/daemon/src/server.rs#L37) は設定された address でそのまま待ち受ける。

**何が問題か**
REST API は local client 向けとされるが、その前提を daemon が強制するのか、認証で守るのかが決まっていない。

**根拠**
コードと OpenAPI で確かめた。

**影響**
状態を変える endpoint を追加した時点で、loopback 以外で待ち受けた daemon は誰からでも操作できる。

**扱い**
daemon-api.md の保留「REST API の到達範囲と認証」へ転記した。

<a id="r-10"></a>
### R-10. rocketpack.md が core-rs の古い compiler の仕様を複写している

**種別: 設計論点** / **確度: 高**

**該当箇所**
rocketpack.md の §3 の `rocketpack.yaml` の説明と、保留「rpf から参照する core-rs 型の扱い」。

**何が問題か**
`targets` の `pattern` で生成対象を選び rpf 1 本から Rust file 1 本を作るという記述は、core-rs の rocketpack-compiler.md の §7 と食い違う。
保留の選択肢も、`RocketPackStruct` を実装した Rust 型を外部型として参照する古い前提に立っていた。

**根拠**
core-rs `5a92929` の rocketpack-compiler.md の §7 と §9、omnikit の `rocketpack.yaml` と `omni_hash.rpf` で確かめた。

**影響**
保留を決めるときに、存在しない選択肢から選ぶことになる。

**扱い**
rocketpack.md の保留「rpf から参照する core-rs 型の扱い」へ転記した。

<a id="r-11"></a>
### R-11. 用語集の使わない別称に別の用語が入り、主要な語が欠けている

**種別: 用語** / **確度: 高**

**該当箇所**
terms.md の用語一覧。
rank の別称に root hash、root hash の別称に FileRef、Profile の別称に memo が入っており、いずれも同じ表の用語である。

**何が問題か**
使わない別称に隣接語を入れると、その語自体を使うなと読める。
設計文書で使う NodeFinder、FileExchanger、FilePublisher、FileSubscriber、MerkleLayer、既知 node、bootstrap node、所在情報、到達先が表にない。

**根拠**
terms.md と設計文書を照合して確かめた。

**影響**
語を引いても定義が見つからない。

**扱い**
terms.md の用語一覧へ転記した（本レビューと同じ変更で修正）。

<a id="r-12"></a>
### R-12. FileExchanger の起動失敗が握り潰される

**種別: 改善提案** / **確度: 高**

**該当箇所**
[file_exchanger.rs:93](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L93) が `start` の `Result` を捨て、[file_exchanger.rs:57](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L57) の `#[allow(unused)]` がその警告を抑える。

**何が問題か**
storage を開けなくても FileExchanger が生成される。

**根拠**
コードで確かめた。

**影響**
FileExchanger が未結線のため顕在化していない。

**扱い**
issues.md の file-exchanger-drops-start-error として起票した。

<a id="r-13"></a>
### R-13. 多重起動の lock が state directory ではなく設定 directory にある

**種別: 改善提案** / **確度: 高**

**該当箇所**
[executor.rs:85](../../daemon/entrypoints/daemon/src/executor.rs#L85) は設定 directory の `axus.lock` を lock する。

**何が問題か**
同じ `state_dir` を指す別の設定 directory から、2 つ目の daemon を起動できる。

**根拠**
コードで確かめた。
2 つの daemon を実際に起動してはいない。

**影響**
同じ署名鍵を持つ 2 つの node が動き、既知 node の書き込みが混ざる。

**扱い**
issues.md の daemon-lock-not-on-state-dir として起票した。

## 5. 確かめて問題がなかったこと

- 互いに接続し合った 2 node が別の Session を残す疑いは、[task_communicator.rs:159](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L159) の規則が両側で同じ Session を選ぶことと、結合試験が通ることで否定した。
- 既知 node の NodeProfile が Session の相手と別人である疑いは、NodeFinder の handshake が cert の公開鍵と照合することで否定した。ただし R-2 の中継は別に残る。
- decode 時の `Vec::with_capacity` が要素数だけで巨大な確保をする疑いは、core-rs の decoder が残りの byte 数を超える要素数を拒否することで frame の大きさまでに留まると確かめた。
- RocksDB の rename が途中で壊れる疑いは、`rename_key` が 1 つの transaction で names を付け替えることで否定した。
- MerkleLayer の rank の意味が符号化側と復号側で食い違う疑いは、多段の round-trip test が通ることで否定した。
- 署名鍵の file が書き込み途中で読まれる疑いは、一時 file に書いてから rename していることで否定した。

## 6. その後
