# Axus 既知の不具合

本書は、コードを読んで確認した明確な不具合を扱う。
設計上の未決事項は対応する設計文書の「設計判断」に置く。

## 1. この文書について

### 1.1 文書間の責務分担

| 文書 | 受け持つもの |
| --- | --- |
| 本書 | コードで確認した不具合の一覧、項目間の関係、修正までの追跡 |
| [design.md](./design.md#11-文書間の責務分担) | 設計判断と実装状況 |
| [terms.md](./terms.md#11-文書間の責務分担) | 語の定義と表記 |

文書族の地図は [design.md](./design.md#11-文書間の責務分担) が持つ。

### 1.2 使い方

- GitHub Issue に起票したら、一覧表の Issue 列に番号を追記する。
- 修正されたら、一覧表の行と子文書を削除する。
- 行番号はコード変更でずれるため、修正時に該当箇所を再確認する。

## 2. 一覧

| 項目 | 深刻度 | 調査日 | Issue |
| --- | --- | --- | --- |
| [アドレスが変わった node が既知 node に重複して残る](./issues/node-profile-rows-keyed-by-uri.md) | 低 | 2026-09-24 | - |

1 件だけであり、ほかの項目との関係はない。
NodeProfile の到達先の真正性（[trust-security.md](./design/trust-security.md#nodeprofile-の到達先の真正性)）を決めると既知 node の保存形式も変わり得るため、あわせて扱う。

## 3. ここに含めていないもの

| 項目 | 参照 |
| --- | --- |
| Session の secure channel がない | [session.md](./design/session.md#5-設計判断) |
| FileExchanger の block 交換 protocol がない | [file-transfer.md](./design/file-transfer.md#7-現状と残作業) |
| 公開と購読を始める REST API がない | [file-transfer.md](./design/file-transfer.md#7-現状と残作業) |
| NodeFinder の能動探索がない | [node-finder.md](./design/node-finder.md#6-設計判断) |
| Web of Trust が未設計である | [trust-security.md](./design/trust-security.md#4-設計判断) |
| Profile と memo が未実装である | [profile-memo.md](./design/profile-memo.md#5-現状と残作業) |
| 長時間 REST 操作の表現が未決である | [daemon-api.md](./design/daemon-api.md#5-設計判断) |
