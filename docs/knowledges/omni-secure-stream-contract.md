# OmniSecureStream V2 は認証モードと context を接続前に固定する

**出所: 文献のみ**（omnius-core-omnikit 0.1.0 / secure connection V2 / POLYVAL 0.7.3 / 2026-10-09）

[knowledges.md](../knowledges.md) の 1 項目。

## 事実

V2 の constructor は context と Anonymous / Mutual の設定を受け取り、peer の申告から設定を選び直さない。
Mutual は双方の profile、役割と鍵合意情報に束縛した Ed25519 の署名を検証する。
両モードとも X25519 の全ゼロ共有秘密を拒否し、鍵確認の完了後に stream を返す。
認証した証明書と DER 公開鍵を取得できるが、期待公開鍵・信頼 policy の照合は呼び出し側が application の通信前に行う。

handshake の frame は 16 KiB 以下とし、生成 decoder の前に固定 schema の構造・深さを制限する。
read_exact の境界で読み、同じ下位 stream を確立後に保持する。
record の Data、KeyUpdate、Close は認証 tag と状態を検査し、暗号化した更新通知から方向別の鍵世代を切り替える。
認証した Close 前の物理 EOF、WriteZero と不正 record は接続全体を fatal にし、両方向の Pending operation を起こす。
操作の途中状態は stream に残るが、複合操作の完了済み prefix は呼び出し側が保持するか接続を破棄する。

AES-GCM 0.11 と GHASH 0.6 は POLYVAL 0.7.3 を保持する。
zeroize feature が有効な POLYVAL 0.7.3 の Drop は保持状態全体を消去する。
AES の鍵 schedule と SHA3 の状態も zeroize feature を使い、方向別秘密・鍵・IV と保持した plaintext は Zeroizing で所有する。
この source 上の消去経路は、一時 Copy やレジスタ残存まで完全に消去したことの証明ではない。

## 根拠

2026-10-09 に V2 の一次ソースと依存版の source を参照した。

- [認証](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/auth.rs#L1)、[明示設定](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/settings.rs#L1)、[transcript](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/transcript.rs#L1)
- [record](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/record.rs#L1)、[stream と fatal](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/stream.rs#L1)、[payload の検査](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/handshake_payload.rs#L1)
- [zeroize の依存指定](../../daemon/refs/core-rs/modules/omnikit/Cargo.toml#L22)、[POLYVAL 0.7.3 の Drop](https://docs.rs/polyval/0.7.3/src/polyval/lib.rs.html#111-119)
- [固定値と基本 I/O 試験](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/runtime_tests.rs#L1)、[normal Drop の回帰検査](../../daemon/refs/core-rs/modules/omnikit/src/service/connection/secure/crypto_tests.rs#L1)

両 workspace の `RUSTC_WRAPPER= cargo test -p omnius-core-omnikit --all-features` は、constructor の攻撃拒否・公開時点、各段階の中断、record・鍵更新と I/O の境界を確認する。
source と feature graph は独立監査で確認した。物理メモリを破棄後に読み出す測定は行っていない。

## 含意

[Session の secure channel](../design/session.md#session-の-secure-channel) は、明示的な Mutual と context、返却された公開鍵の照合を統合時に要求する。
[鍵更新](../design/session.md#鍵を方向ごとに通信中に更新する) は application の frame 境界から独立して扱う。
core-rs V2 の受け入れ確認と、期待公開鍵の照合を伴う Axus の Session V2 統合は別の作業である。
