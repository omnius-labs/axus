# 署名鍵の識別子が `"TODO"` 固定である

**深刻度: 中**（設計論点に依存）

調査日 2026-09-02 / 対象コミット `8970f441df9629246e5c6b536334683f79f3953a`

[issues.md](../issues.md) の 1 項目。

## 症状

すべての AxusService が同じ識別子から署名鍵を生成する。

## 該当箇所

[service.rs:53](../../daemon/modules/engine/src/service.rs#L53) が固定文字列を OmniSigner に渡している。

```rust
let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "TODO")?);
```

[node_finder.rs:70](../../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L70) は、署名鍵と結び付かない NodeProfile.id を生成する。

## 原因

node identity の設計が決まる前の仮値が残っている。

## 影響

Session 層の相互認証で node ごとの鍵を識別できない。
NodeProfile.id と署名鍵を同じ主体として検証できない。

## 対応方針

[trust-security.md](../design/trust-security.md#4-設計判断) で node identity と署名鍵の関係を決める。
決定した identity に基づいて signer の識別子と永続化方法を実装する。
node ごとに異なる鍵が生成または復元されることを test する。
