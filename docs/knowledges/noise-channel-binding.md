# Noise の暗号化と Ed25519 identity の認証は別の contract である

**出所: 文献のみ**（Noise revision 34 / snow 0.10.0 / 2026-10-08）

[knowledges.md](../knowledges.md) の 1 項目。

## 事実

Noise の NN pattern は ephemeral DH から暗号化鍵を作るが、相手の identity を認証しない。
XX pattern は双方の static DH key の所持を認証する。
Noise の `25519` は X25519 の DH key であり、Ed25519 の署名鍵をそのまま認証するものではない。

Noise は最終 handshake hash を使う channel binding を規定する。
handshake が終わった後に hash へ署名することで、別の Noise 接続へ認証 token を流用することを防げる。
`snow::HandshakeState` は hash と相手の static key を取得する API を提供するが、Ed25519 による追加認証、公開鍵の信頼 policy と非同期 I/O はアプリケーションの責務である。

Noise message は暗号文を含めて 65,535 byte 以下、transport の plaintext は認証 tag の 16 byte を除いた 65,519 byte 以下である。
大きな上位 message を運ぶには record への分割が必要になる。
仕様の hash の組み合わせには SHA-256、SHA-512、BLAKE2s、BLAKE2b があり、SHA3-256 は含まれない。

snow は方向別 nonce の状態と枯渇を扱うが、鍵使用量の上限と rekey の実施条件・同期はアプリケーションが定める。
Noise の `25519` 仕様は全ゼロの DH 出力を許し、拒否も許容するため、全ゼロ拒否を Noise 一般の必須要件とは扱えない。

## 根拠

2026-10-08 に次の一次資料を参照した。

- [Noise revision 34 §3](https://noiseprotocol.org/noise.html#message-format)、[§7.5 の NN と XX](https://noiseprotocol.org/noise.html#interactive-handshake-patterns-fundamental)
- [§11.2 channel binding](https://noiseprotocol.org/noise.html#channel-binding)、[§11.3 rekey](https://noiseprotocol.org/noise.html#rekey)、[§12 の関数](https://noiseprotocol.org/noise.html#dh-functions-cipher-functions-and-hash-functions)、[§13 アプリケーションの責務](https://noiseprotocol.org/noise.html#application-responsibilities)
- [snow 0.10.0 HandshakeState](https://docs.rs/snow/0.10.0/snow/struct.HandshakeState.html#method.get_handshake_hash)、[TransportState の長さと rekey](https://docs.rs/snow/0.10.0/snow/struct.TransportState.html#method.write_message)

追加認証の動作と安全性、Axus との相互接続、性能と工数は検証していない。

## 含意

[Session の secure channel](../design/session.md#session-の-secure-channel) の比較では、Noise の暗号手順を再利用する範囲と、Ed25519 identity、I/O、鍵使用量の管理を自分たちで設計する範囲を分ける。
NN と最終 hash への用途・役割付き署名は、既存 identity を維持する候補になるが、追加した認証 protocol 全体の安全性が既に検証されたとは扱わない。
