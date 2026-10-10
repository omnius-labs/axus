# Session の secure channel の実装計画

## 1. この計画について

本書は、core-rs の `OmniSecureStream` の handshake を書き換え、Axus の Session をその上に載せ替える作業を扱う。
作業は core-rs と Axus の 2 つの repository にまたがる。

### 1.1 文書間の責務分担

| 文書 | 受け持つもの |
| --- | --- |
| 本書 | この作業の完了条件、手順、受け入れ検証 |
| [session.md](../design/session.md#session-の-secure-channel) | Session が secure channel に求める性質と、その理由 |
| [trust-security.md](../design/trust-security.md#3-保証の分担と脅威) | secure channel が保証するものと保証しないもの |
| [core-rs の DESIGN.md](../../daemon/refs/core-rs/docs/DESIGN.md#62-secure-connection) | `OmniSecureStream` の handshake の構成、鍵導出、frame の暗号化 |
| [terms.md](../terms.md#t-node-profile-vs-session) | Session と NodeProfile の語の境界 |
| [issues.md](../issues.md) | 本書で扱わない既知の不具合 |

文書族の地図は [design.md](../design.md#11-文書間の責務分担) が持つ。

### 1.2 作成日と対象コミット

作成日は 2026-10-10 である。
対象コミットは Axus が `469f6fc`、core-rs が submodule の指す `831a476` である。

## 2. 目的と完了条件

信頼できない network でも、Session の相手が署名者本人であり、第三者が Session の通信を読むことも書き換えることもできない状態にする。

### 2.1 完了条件

1. core-rs の `OmniSecureStream` は、中継の途中で署名を別の鍵によるものへ差し替えられた handshake を失敗させる。
2. core-rs の `OmniSecureStream` は、記録した handshake の message を再送する相手との handshake を失敗させる。
3. core-rs の `OmniSecureStream` は、profile または一時公開鍵を書き換えられた handshake を失敗させる。
4. core-rs の `OmniSecureStream` は、署名を必須に指定した側で、署名を送らない相手との handshake を失敗させる。
5. core-rs の `cargo make lint` と omnikit の test が全件通る。
6. Axus の Session が確立後に送る message の平文は、TCP を流れる byte 列に現れない。
7. Axus の `daemon/rpfs` と `daemon/modules` に `V1ChallengeMessage` と `V1SignatureMessage` が残っていない。
8. Axus の `cargo make lint` と engine の test が全件通り、2 node と 3 node の結合試験を含む。
9. core-cs と core-swift の GitHub に、`OmniSecureStream` が core-rs と通信できないことを記した Issue がある。
10. [session.md](../design/session.md#6-現状と残作業) と [trust-security.md](../design/trust-security.md#5-現状と残作業) に、secure channel を実装していないという記述がない。
11. [design.md](../design.md#62-ロードマップ) のロードマップに、secure channel の決定と RocketPack 型の移行を内容とする行がない。
12. core-rs の `OmniSecureStream` は、一時公開鍵の作成時刻が受け取った側の時刻から許容幅を超えて離れた handshake を失敗させる。
13. core-rs の `OmniSecureStream` は、相手が frame の境界で接続を閉じたときに EOF を返し、frame の途中で閉じたときにエラーを返す。

### 2.2 非目標

| 項目 | 外した理由 | 残す先 |
| --- | --- | --- |
| NodeProfile の到達先の真正性 | secure channel の実装で偽の到達先の影響が変わり、変更箇所も NodeFinder の wire format と保存形式に分かれる | [trust-security.md](../design/trust-security.md#nodeprofile-の到達先の真正性) |
| 既知 node の重複 | 到達先の真正性の方式で保存形式が変わり得る | [node-profile-rows-keyed-by-uri](../issues/node-profile-rows-keyed-by-uri.md) |
| 確立後の送受信と受理待ちの期限 | handshake より後の処理が対象で、この作業の変更と独立している | [session-accept-and-message-without-deadline](../issues/session-accept-and-message-without-deadline.md) |
| core-cs と core-swift の handshake の書き換え | Axus の daemon は Rust だけで動き、handshake を Axus で使い終えてから展開するほうが修正箇所が少ない | 手順 3 で起票する各 repository の Issue |
| Session の version 選択の規則 | version が V1 だけで、規則の違いが交渉結果に現れない | [session.md](../design/session.md#session-の-version-選択) |

## 3. 現状

Axus の Session は、平文の FramedStream で handshake を行う。
発信側は [connector.rs:46](../../daemon/modules/engine/src/core/session/connector.rs#L46)、受理側は [accepter.rs:209](../../daemon/modules/engine/src/core/session/accepter.rs#L209) の `handshake` で、相手が選んだ 32 byte の nonce に署名し、相手の署名を [connector.rs:66](../../daemon/modules/engine/src/core/session/connector.rs#L66) と [accepter.rs:229](../../daemon/modules/engine/src/core/session/accepter.rs#L229) で検証する。
この署名は鍵合意を伴わず、確立後の message も平文で流れる。
challenge と signature の型は [session.rpf:12](../../daemon/rpfs/session.rpf#L12) と [message.rs:16](../../daemon/modules/engine/src/core/session/message.rs#L16)、codec は [protocol/session.rs:24](../../daemon/modules/engine/src/protocol/session.rs#L24) にある。

connection は TCP stream を FramedStream に包んで返す。
[tcp/connector.rs:21](../../daemon/modules/engine/src/base/connection/tcp/connector.rs#L21) の `ConnectionTcpConnector::connect` と [tcp/accepter.rs:21](../../daemon/modules/engine/src/base/connection/tcp/accepter.rs#L21) の `ConnectionTcpAccepter::accept` の戻り値が `FramedStream` であるため、Session 層は `AsyncRead` と `AsyncWrite` を要求する `OmniSecureStream` を間に挟めない。

NodeFinder は [task_communicator.rs:167](../../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L167) で、相手の NodeProfile の公開鍵を `Session` の `cert` の公開鍵と照合する。
`OmniSecureStream` が相手について公開するのは [stream.rs:94](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L94) の `sign_id` の文字列だけであり、公開鍵を取り出せない。

core-rs の handshake は [auth.rs:76](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L76) の `Authenticator::auth` にある。
署名の対象は [auth.rs:180](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L180) の `gen_hash` が作る hash で、署名者自身の profile と一時公開鍵だけを含み、接頭辞を持たない。
相手の署名は [auth.rs:116](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L116) で、相手が `AuthType::Sign` を申告した場合にだけ検証する。
鍵導出は [auth.rs:139](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L139) で双方の `session_id` の XOR を salt とし、[auth.rs:148](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L148) で info を空にして展開する。
鍵確認の message はない。
一時公開鍵の `created_time` は [auth.rs:189](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L189) で署名の対象に入るが、受け取った側は自分の時刻と比べない。
[omni_agreement.rs:38](../../daemon/refs/core-rs/modules/omnikit/src/model/omni_agreement.rs#L38) の `gen_secret` は、X25519 の結果がすべて 0 になる公開鍵を拒否しない。

暗号化した frame を読む [stream.rs:103](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L103) の `poll_read` は、下位の stream が 0 byte を返しても受信の状態を終わらせず、同じ読み取りを繰り返す。
loopback の TCP で handshake と一方向の frame の送受信を成功させた後に相手を閉じると、受信は EOF を返さず、1 秒待っても戻らない。
Axus の [core/session.rs:239](../../daemon/modules/engine/src/core/session.rs#L239) 以降の test は相手の切断を受信のエラーとして検出しており、この挙動のままでは維持できない。

現状の Session は [session.md](../design/session.md#session-の-secure-channel) が求める 4 つの性質をどれも満たさず、core-rs の `OmniSecureStream` をそのまま適用しても満たせない。

## 4. 変更方針

core-rs では handshake の層を書き換える。
暗号化した frame を読み書きする層は、切断を EOF として返す修正を除いて変えない。
Axus では connection が byte 列を返すように境界を移し、Session 層が `OmniSecureStream` と FramedStream を順に重ねる。
Session の HelloMessage、用途の要求と結果は、重ねた FramedStream で交換する。

```text
NodeFinder / FileExchanger の message
FramedStream
OmniSecureStream
TCP
```

handshake の期限と frame 上限は TCP 接続の直後から数えるため、`OmniSecureStream` の handshake もその範囲に入る。

### 4.1 前提とする決定

| 決定 | 正本 |
| --- | --- |
| Session は `OmniSecureStream` で保護し、双方が署名を必須とする | [session.md](../design/session.md#session-の-secure-channel) |
| 署名は接頭辞と接続の向きを含む handshake の transcript を対象にし、鍵導出は transcript に依存し、双方の署名の公開鍵を対象に含む鍵確認を交換する | [session.md](../design/session.md#session-の-secure-channel) |
| 一時公開鍵の作成時刻と受け取った側の時刻の差が 5 分を超える handshake を拒否する | [session.md](../design/session.md#session-の-secure-channel) |
| version と用途は暗号化された stream で交換する | [session.md](../design/session.md#3-接続と確立) |
| 旧い V1 の手順を廃止し、version を増やさずに置き換える | [session.md](../design/session.md#session-の-secure-channel) |
| handshake 全体の期限は 10 秒、同時数は 64 本以下とする | [session.md](../design/session.md#handshake-の期限と並列度) |
| handshake 中の frame は 16 KiB 以下とする | [session.md](../design/session.md#用途に応じた-frame-上限) |
| NodeFinder は NodeProfile の公開鍵を Session の cert と照合する | [trust-security.md](../design/trust-security.md#node-id-は公開鍵から導出する) |

### 4.2 影響範囲

core-rs では omnikit の `service/connection/secure` と `omni_secure.rpf`、`OmniSecureStream::new` の引数が変わり、相手の cert を返す関数が加わる。
`OmniSecureStream` の wire format は互換でなくなり、core-cs と core-swift の実装とは通信できなくなる。

Axus では `base/connection` の TCP の trait、`core/session`、`rpfs/session.rpf` とその生成物、`protocol/session.rs` の codec と test が変わる。
Session の wire format は互換でなくなり、変更前の daemon とは Session を確立できない。
設定 file、REST API、SQLite と RocksDB の保存形式は変わらない。

`Session` の `cert` と `stream` の型は変えないため、NodeFinder と FileExchanger の message の処理は変更しない。
`SessionConnector` と `SessionAccepter` の constructor に clock が加わり、[service.rs:87](../../daemon/modules/engine/src/service.rs#L87) をはじめとする呼び出し側が変わる。
Session の語の境界は [terms.md](../terms.md#t-node-profile-vs-session) に反映してあり、追加する語はない。

## 5. 手順

| 番号 | 内容 | 依存 | 状態 |
| --- | --- | --- | --- |
| [1](#s-1) | core-rs の設計文書に handshake を定める | | 完了 |
| [2](#s-2) | core-rs の handshake を書き換える | 1 | 完了 |
| [3](#s-3) | core-cs と core-swift に非互換を起票する | 2 | 完了 |
| [4](#s-4) | Axus の connection が byte 列を返す | | 進行中 |
| [7](#s-7) | core-rs の `OmniSecureStream` が切断を EOF として返す | 2 | 進行中 |
| [5](#s-5) | Axus の Session を `OmniSecureStream` に載せ替える | 2、4、7 | 未着手 |
| [6](#s-6) | Axus の設計文書を実装に合わせる | 5 | 未着手 |

<a id="s-1"></a>
### 手順 1. core-rs の設計文書に handshake を定める

**変更する箇所**
[DESIGN.md:202](../../daemon/refs/core-rs/docs/DESIGN.md#L202) の「secure connection」と、[DESIGN.md:358](../../daemon/refs/core-rs/docs/DESIGN.md#L358) の「署名 preimage を意味的フィールドへ固定する」。

**やること**
[session.md](../design/session.md#session-の-secure-channel) の 4 つの性質を満たす handshake を、message の順序、transcript に含める値と連結順、接続の向きの表し方、署名の接頭辞と preimage、HKDF の salt と info、鍵確認の message とその対象に双方の署名の公開鍵を含める方法、失敗時の挙動まで定める。
あわせて次の 4 点を定める。

- 呼び出し側が相手の署名を必須に指定でき、指定した側は署名のない相手を拒否する。`AuthType::None` は残す。
- handshake の完了後に、呼び出し側が相手の cert を取得できる。
- X25519 の結果がすべて 0 になる公開鍵を拒否する。
- 呼び出し側が作成時刻の許容幅を指定でき、受け取った側は自分の時刻との差が許容幅を超える一時公開鍵を拒否する。

[decoder.rs](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/decoder.rs#L1) と [stream.rs:16](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L16) 以降の読み書きを読み、frame の層を変更せずに新しい handshake の鍵で使えることを確かめる。
新しい handshake の鍵を使うために frame の層の変更が必要だと分かった場合は、本書の手順に含めず、作業を止めて範囲を決め直す。

**完了条件**
core-rs の DESIGN.md の §6.2 に上の各項目と、frame の層を変更せずに新しい handshake の鍵で使えることを確かめた結果があり、旧い署名 preimage の決定が新しい内容に置き換わっている。
本体と異なる model family の verifier が、この設計に `pass` を返している。

<a id="s-2"></a>
### 手順 2. core-rs の handshake を書き換える

**変更する箇所**
[auth.rs:76](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L76) の `auth` と [auth.rs:180](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L180) の `gen_hash`、[stream.rs:70](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L70) の `OmniSecureStream::new` と [stream.rs:94](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L94) の `sign_id`、[omni_secure.rpf:9](../../daemon/refs/core-rs/modules/omnikit/rpfs/omni_secure.rpf#L9) の message 定義、[omni_agreement.rs:38](../../daemon/refs/core-rs/modules/omnikit/src/model/omni_agreement.rs#L38) の `gen_secret`。

**やること**
手順 1 の設計どおりに handshake を実装する。
test を 5 件追加する。
`handshake_rejects_substituted_signature` は、中継者が署名を自分の鍵によるものへ差し替え、ほかの message をそのまま中継した handshake を、`handshake_rejects_replayed_messages` は記録した message の再送を、`handshake_rejects_tampered_profile_or_key` は profile と一時公開鍵の書き換えを、`handshake_rejects_unsigned_peer_when_signature_is_required` は署名のない相手を、`handshake_rejects_agreement_key_outside_time_tolerance` は作成時刻が許容幅より古い一時公開鍵と新しい一時公開鍵を、それぞれ拒否することを守る。

**完了条件**
追加した 5 件と omnikit の既存 test が全件通り、core-rs の `cargo make lint` が通る。
変更が core-rs の main に merge されている。

<a id="s-3"></a>
### 手順 3. core-cs と core-swift に非互換を起票する

**変更する箇所**
GitHub の `omnius-labs/core-cs` と `omnius-labs/core-swift` の Issue。
対象の実装は core-cs の `src/Core.Omnikit/Connections/Secure/V1/Authenticator.cs` と、core-swift の `Sources/Omnikit/Service/Connections/Secure/Authenticator.swift` である。

**やること**
`OmniSecureStream` の handshake が core-rs と通信できないこと、追従に必要な設計が core-rs の DESIGN.md の §6.2 にあること、手順 2 の merge commit を Issue に書く。

**完了条件**
両 repository に Issue があり、どちらの本文も、core-rs と通信できないことの説明、core-rs の DESIGN.md の §6.2 への参照、core-rs の merge commit を含む。

<a id="s-4"></a>
### 手順 4. Axus の connection が byte 列を返す

**変更する箇所**
[tcp/connector.rs:21](../../daemon/modules/engine/src/base/connection/tcp/connector.rs#L21) の `ConnectionTcpConnector::connect`、[tcp/accepter.rs:21](../../daemon/modules/engine/src/base/connection/tcp/accepter.rs#L21) の `ConnectionTcpAccepter::accept`、それぞれの実装である [tcp/connector.rs:42](../../daemon/modules/engine/src/base/connection/tcp/connector.rs#L42) と [tcp/accepter.rs:91](../../daemon/modules/engine/src/base/connection/tcp/accepter.rs#L91)、test 用の実装がある [core/session.rs:582](../../daemon/modules/engine/src/core/session.rs#L582)。

**やること**
2 つの trait の戻り値を、`AsyncRead` と `AsyncWrite` を実装する byte 列に変える。
FramedStream の生成を [connector.rs:46](../../daemon/modules/engine/src/core/session/connector.rs#L46) と [accepter.rs:209](../../daemon/modules/engine/src/core/session/accepter.rs#L209) の `handshake` の入口へ移す。
handshake の内容は変えない。

**完了条件**
2 つの trait の戻り値に `FramedStream` が現れず、engine の既存 test が全件通る。

<a id="s-7"></a>
### 手順 7. core-rs の `OmniSecureStream` が切断を EOF として返す

**変更する箇所**
[stream.rs:103](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L103) の `poll_read` と、core-rs の [DESIGN.md:202](../../daemon/refs/core-rs/docs/DESIGN.md#L202) の「secure connection」。

**やること**
`poll_read` は、下位の stream が frame の境界で 0 byte を返したら EOF を返し、frame の途中で 0 byte を返したら `UnexpectedEof` のエラーを返す。
frame の形式、暗号化の方式、書き込みの処理、handshake は変えない。
test を追加し、frame の境界での切断が EOF になること、header の途中と body の途中での切断がエラーになることを守る。
切断時の挙動を core-rs の DESIGN.md の §6.2 に書く。

**完了条件**
追加した test と omnikit の既存 test が全件通り、core-rs の `cargo make lint` が通る。
変更が core-rs の main に merge されている。

<a id="s-5"></a>
### 手順 5. Axus の Session を `OmniSecureStream` に載せ替える

**変更する箇所**
submodule の `daemon/refs/core-rs`、[connector.rs:29](../../daemon/modules/engine/src/core/session/connector.rs#L29) の `SessionConnector::new` と [accepter.rs:41](../../daemon/modules/engine/src/core/session/accepter.rs#L41) の `SessionAccepter::new`、[connector.rs:46](../../daemon/modules/engine/src/core/session/connector.rs#L46) と [accepter.rs:209](../../daemon/modules/engine/src/core/session/accepter.rs#L209) の `handshake`、[session.rpf:12](../../daemon/rpfs/session.rpf#L12) の `V1ChallengeMessage` と `V1SignatureMessage` とその生成物、[message.rs:16](../../daemon/modules/engine/src/core/session/message.rs#L16)、[protocol/session.rs:24](../../daemon/modules/engine/src/protocol/session.rs#L24) の codec、[protocol/tests.rs:163](../../daemon/modules/engine/src/protocol/tests.rs#L163) と [protocol/tests/legacy/session.rs](../../daemon/modules/engine/src/protocol/tests/legacy/session.rs#L1) の互換 test、[core/session.rs:239](../../daemon/modules/engine/src/core/session.rs#L239) 以降の handshake の test。

**やること**
submodule を手順 7 の merge commit へ進める。
`SessionConnector` と `SessionAccepter` は、`OmniSecureStream` に渡す clock を constructor で受け取る。
`handshake` の先頭で、署名を必須にし、作成時刻の許容幅を 5 分にした `OmniSecureStream` を byte 列に重ね、その上に FramedStream を作る。
challenge と signature の交換を削除し、HelloMessage、用途の要求と結果を FramedStream で交換する。
`Session` の `cert` には `OmniSecureStream` から得た相手の cert を入れる。
`V1ChallengeMessage` と `V1SignatureMessage` の型、codec、互換 test を削除する。
期限と frame 上限の既存 test は、新しい handshake の message 列に合わせて書き直す。
test を 1 件追加する。
`established_session_does_not_expose_plaintext` は、確立後に送った message の平文が TCP の byte 列に現れないことを守る。

**完了条件**
追加した 1 件と engine の test が全件通り、`cargo make lint` が通る。
`rg 'V1ChallengeMessage|V1SignatureMessage' daemon/rpfs daemon/modules` の出力が空である。

<a id="s-6"></a>
### 手順 6. Axus の設計文書を実装に合わせる

**変更する箇所**
[session.md:205](../design/session.md#L205) 以降の「現状と残作業」、[trust-security.md:131](../design/trust-security.md#L131) 以降の「現状と残作業」、[design.md:157](../design.md#L157) と [design.md:161](../design.md#L161) のロードマップの行、[issues.md:42](../issues.md#L42) と [issues.md:46](../issues.md#L46) の行。

**やること**
2 つの「現状と残作業」を、secure channel を実装した状態の記述に改める。
ロードマップの行 1 を NodeProfile の到達先の真正性だけの内容に改め、RocketPack 型の移行の行を削除する。
`issues.md` の §3 から、secure channel がないことと認証が接続に束縛されていないことの 2 行を削除する。

**完了条件**
`rg '実装していない' docs/design/session.md` の出力が空である。
`docs/design.md` のロードマップに「secure channel」と「RocketPack」を含む行がなく、`docs/issues.md` に `session-の-secure-channel` へのリンクがない。

## 6. 受け入れ検証

core-rs のコマンドは core-rs の作業 tree の直下で、Axus のコマンドは Axus の `daemon` で実行する。
条件 1 から 4、6、12、13 の test は、この作業で追加する。

| 条件 | コマンド | 期待する結果 |
| --- | --- | --- |
| 1 | `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit handshake_rejects_substituted_signature` | 1 件が通る |
| 2 | `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit handshake_rejects_replayed_messages` | 1 件が通る |
| 3 | `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit handshake_rejects_tampered_profile_or_key` | 1 件が通る |
| 4 | `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit handshake_rejects_unsigned_peer_when_signature_is_required` | 1 件が通る |
| 5 | `RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo test -p omnius-core-omnikit` | lint が成功し、test の失敗が 0 件である |
| 6 | `RUSTC_WRAPPER= cargo test -p omnius-axus-engine established_session_does_not_expose_plaintext` | 1 件が通る |
| 7 | `rg 'V1ChallengeMessage\|V1SignatureMessage' rpfs modules` | 出力が空である |
| 8 | `RUSTC_WRAPPER= cargo make lint && RUSTC_WRAPPER= cargo test -p omnius-axus-engine` | lint が成功し、test の失敗が 0 件で、`node_finds_asset_key_owner_through_bootstrap_node` と `node_finds_and_dials_an_owner_known_only_through_another_node` が通る |
| 9 | `gh issue list -R omnius-labs/core-cs --search OmniSecureStream --json number,title,body && gh issue list -R omnius-labs/core-swift --search OmniSecureStream --json number,title,body` | どちらの出力にも、core-rs と通信できないことの説明、core-rs の DESIGN.md の §6.2 への参照、手順 2 の merge commit の 3 つを本文に含む Issue が 1 件ある |
| 10 | `rg '実装していない\|暗号化していない' ../docs/design/session.md ../docs/design/trust-security.md` | 出力が空である |
| 11 | `rg '^. [0-9] .*secure channel' ../docs/design.md; rg '^. [0-9] .*RocketPack' ../docs/design.md` | どちらの出力も空である |
| 12 | `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit handshake_rejects_agreement_key_outside_time_tolerance` | 1 件が通る |
| 13 | `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit secure::stream` | 切断の test を含めて失敗が 0 件である |
