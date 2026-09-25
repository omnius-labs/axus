# Profile と memo の設計

## 1. このドキュメントについて

本書は上位 service が公開データを Profile から参照し、memo がその Profile を配布するための責務境界を扱う。
語の定義は [terms.md](../terms.md) が正とする。

### 1.1 文書間の責務分担

| 文書または正本 | 受け持つもの |
| --- | --- |
| [design.md](../design.md#11-文書間の責務分担) | 全体構成、他の関心事との境界、横断的な依存関係 |
| 本書 | Profile の参照、memo の配布、上位 service との境界 |
| [file-transfer.md](./file-transfer.md#2-責務と境界) | file の公開、探索、block 転送 |
| [trust-security.md](./trust-security.md#4-設計判断) | Profile の主体と信頼の判断 |

### 1.2 本書の時制について

本文は完成形の設計を記述する。
実装状況と残作業は §5 に集約する。

## 2. 責務と境界

Profile は投稿本文や添付 byte 列を直接埋め込まず、公開済み file の FileRef を保持する。
投稿群をまとめた file と利用者が個別に公開した file は、どちらも先に FilePublisher へ渡して root hash を得る。
上位 service 層はその結果から FileRef を構築し、Profile の分類規則に従って登録する。

この構造により、大きなデータの配布と Profile の更新を同じ protocol に重ねずに済む。
Profile の真正性を検証しても、参照先 file の block hash 検証は省略しない。

## 3. 層ごとの責務

| 層 | 責務 | 知らないもの |
| --- | --- | --- |
| FilePublisher と FileExchanger | file の公開、探索、block 転送 | 投稿、Profile の意味 |
| MemoExchanger | Profile の探索、交換、配布 | 投稿本文、添付の解釈 |
| 上位 service 層 | 投稿データの構築、FileRef の分類、Profile の生成と更新 | block 転送の詳細 |

下位層は Profile を opaque な payload として扱い、上位層だけが FileRef の意味を解釈する。
この境界により、掲示板や timeline のデータ model を変えても、memo の探索と配布を作り直さずに済む。

## 4. 設計判断

### 4.1 決定済み

#### Profile は公開データを FileRef で参照する

**決定**
Profile は投稿本文と添付を埋め込まず、FilePublisher で公開したデータの FileRef を保持する。
MemoExchanger は Profile の意味を解釈せず、探索、交換、配布だけを担う。

**理由**
内容配布を既存の file protocol に集約し、Profile と掲示板の model を transport から分離するためである。

### 4.2 保留

#### Profile の形式と更新

**現状**
Profile が FileRef 群を持ち、memo が Profile を配布する責務境界だけを定めている。

候補は次の 2 つである。

1. version 付きの immutable Profile を連鎖させると履歴検証が容易だが、最新版探索と storage 回収が必要になる。
2. 主体ごとの mutable な最新版を配布すると取得は単純だが、競合、rollback、replay の規則が必要になる。

**なぜ今決めないか**
identity と Web of Trust の署名対象が決まるまで、Profile の真正性と競合解決を定義できないためである。

**決める条件**
Profile 型、MemoRef、MemoExchanger のいずれかを protocol surface として実装する前に決める。

## 5. 現状と残作業

FileRef 型はあるが、memo と MemoRef の module は空であり、Profile 型と上位 service 層はない。
identity と trust の判断の後に、Profile の形式、更新規則、memo の protocol surface を定義する。
