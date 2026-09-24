# trust と security の設計

## 1. このドキュメントについて

本書は Session、Merkle hash、Web of Trust がそれぞれ保証する範囲と、node identity および脅威への境界を扱う。
語の定義は [terms.md](../terms.md) が正とする。

### 1.1 文書間の責務分担

| 文書または正本 | 受け持つもの |
| --- | --- |
| [design.md](../design.md#11-文書間の責務分担) | 全体構成、他の関心事との境界、横断的な依存関係 |
| 本書 | security の保証範囲、脅威、identity、Web of Trust の設計判断 |
| [session.md](./session.md#2-責務と境界) | Session の確立と用途分離 |
| [file-transfer.md](./file-transfer.md#5-接続相手と-block-の検証) | block の内容 hash 検証 |
| [profile-memo.md](./profile-memo.md#2-責務と境界) | Profile の payload と memo の責務境界 |

### 1.2 本書の時制について

本文は完成形の設計を記述する。
実装状況と残作業は §5 に集約する。

## 2. 責務と境界

認証、暗号化、内容 hash、信頼評価は別の保証であり、どれか 1 つで他を代替しない。
本書は各保証の境界と、そこから導かれる identity および Web of Trust の保留を持つ。
個別の Session protocol、file protocol、Profile の payload 構造は対応する設計文書が持つ。

## 3. 保証の分担と脅威

| 機構 | 保証するもの | 保証しないもの |
| --- | --- | --- |
| Session の challenge signature | 相手が応答に使った秘密鍵を所持すること | 相手への信頼 |
| NodeFinder の handshake での公開鍵の照合 | 相手の NodeProfile と node ID が Session の相手のものであること | NodeProfile の到達先の真正性 |
| FramedStream | message 境界 | 通信内容の機密性と改竄検出 |
| Merkle hash | 期待する hash に対する block 内容の一致 | 提供者の信頼性、検索結果の正当性 |
| Web of Trust | §4 で決める信頼 policy | transport の暗号化と block の内容検証 |

Session 認証が成功しても、相手の NodeProfile と公開内容を信頼できるとは限らない。
この前提により、identity、暗号化、内容検証、信頼評価を別々に実装できる。

node ID は公開鍵から導出するが、鍵は低コストで生成できるため、攻撃者は AssetKey に近い ID を集める Sybil 攻撃と Eclipse 攻撃を行える。
既知 node と伝播情報の件数上限は資源消費を抑えるが、攻撃者の情報だけが残ることは防がない。
Web of Trust はこの信頼選別を担うが、transport の盗聴と改竄には別の secure channel が必要である。

NodeFinder の中継は情報を増幅し得るため、TTL、件数、接続数、message size の上限を protocol の入力境界で強制する。
FileExchanger は受信 block を hash 検証し、Profile 交換は署名と version 検証を通す。

## 4. 設計判断

### 4.1 決定済み

Session、block、Profile の個別の決定は各設計文書が正とする。

#### node ID は公開鍵から導出する

**決定**
NodeProfile は node ID ではなく公開鍵を持ち、node ID は公開鍵の SHA3-256 hash として導出する。
署名鍵は state directory に保存し、再起動後も同じ鍵と node ID を使う。
NodeFinder の handshake は、相手の NodeProfile の公開鍵が Session の cert の公開鍵と一致しない場合に接続を拒否する。

**理由**
node ID を公開鍵から導出すると、ID と鍵の対応を別に証明しなくても、Session の署名検証だけで相手の ID が確定する。
ID を保持せずに導出するため、ID と鍵が食い違う NodeProfile を表現できない。
鍵を rotation すると node ID も変わるが、ID を引き継ぐ要件は現時点でない。

### 4.2 保留

#### NodeProfile の到達先の真正性

**現状**
NodeProfile の到達先には署名がなく、第三者はある node の公開鍵に偽の到達先を付けて配布できる。
接続すると handshake で公開鍵を照合するためなりすましは成立しないが、接続の失敗を誘って特定の node を探索から外せる。

候補は次の 2 つである。

1. NodeProfile 全体に本人が署名すると、配布の経路によらず到達先を検証できるが、NodeProfile の wire format に署名と更新順序の field が必要になる。
2. 接続に成功した到達先だけを保存して配布すると wire format は変わらないが、未検証の到達先を最初に試す段階は残る。

**なぜ今決めないか**
自 node の到達先を広告する処理がまだなく、偽の到達先が探索に与える影響を測れないためである。

**決める条件**
自 node の到達先を広告した後、信頼できない network で NodeFinder を動かす前に決める。

#### Web of Trust の単位と伝播

**現状**
README は Web of Trust による検索と公開の保護を掲げるが、信頼対象と score の計算規則を定めていない。

候補は次の 2 つである。

1. 直接署名だけを使うと説明と失効が単純だが、未知の主体を評価できない。
2. 推移的な trust を使うと discovery 範囲が広がるが、深さ、減衰、循環、Sybil 耐性を規定する必要がある。

**なぜ今決めないか**
信頼を適用する API が確定していないためである。

**決める条件**
検索結果または Profile を trust score で選別する最初の API を定義する前に決める。

## 5. 現状と残作業

Session は challenge signature を交換するが、通信を暗号化していない。
署名鍵は state directory に保存し、node ID は公開鍵から導出して NodeFinder の handshake で照合する。
NodeProfile の到達先の真正性と Web of Trust の policy を決め、信頼できない network へ適用する前に secure channel を選ぶ。
