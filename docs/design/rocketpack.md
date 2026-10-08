# RocketPack 型の設計

## 1. このドキュメントについて

本書は RocketPack で符号化する型の正、生成物、手書き実装からの移行規約を扱う。
語の定義は [terms.md](../terms.md) が正とする。

### 1.1 文書間の責務分担

| 文書または正本 | 受け持つもの |
| --- | --- |
| [design.md](../design.md#11-文書間の責務分担) | 全体構成、他の関心事との境界、横断的な依存関係 |
| 本書 | Axus が採用する RocketPack 型生成と移行の設計 |
| [rocketpack-compiler.md](../../daemon/refs/core-rs/docs/design/rocketpack-compiler.md) | `rocketpack.yaml` の書式、外部型の参照、生成される module の構成 |
| [core-rs の DESIGN.md](../../daemon/refs/core-rs/docs/DESIGN.md#5-rocketpack) | RocketPack の長さ制約と decode 時の検査 |
| [rocketpack-compiler](../../daemon/refs/core-rs/entrypoints/rocketpack-compiler) | compiler の実装 |
| [rocketpack-compiled-example](../../daemon/refs/core-rs/entrypoints/rocketpack-compiled-example) | compiler 設定と生成物の例 |
| `.rpf` file | 型、field 番号、外部型参照の正 |

### 1.2 本書の時制について

本文は完成形の設計を記述する。
実装状況と残作業は §6 に集約する。

## 2. 責務と境界

Axus は rpf を通信型の正として、Rust の型宣言と `RocketPackStruct` 実装を生成する。
生成型をそのまま扱える AssetKey、FileRef、MerkleLayer は生成型を使う。
DataMessage の `Arc` と `HashMap`、NodeProfile の `OmniAddr` と ID キャッシュのように Rust 上の表現が異なる場合は、手書きの domain 型を別に保持する。
domain 型は `RocketPackStruct` を実装せず、[protocol module](../../daemon/modules/engine/src/protocol) の専用 codec が生成型との相互変換と符号化・復号を担当する。
Session の整数 enum と nonce 配列も、専用 codec が生成型との対応を持つ。
compiler の内部設計は `core-rs` が持ち、本書は Axus から見た入出力と移行規約だけを扱う。

## 3. 生成の入力と出力

```mermaid
flowchart LR
    Rpf[rpf file]
    Conf[rocketpack.yaml]
    Compiler[rocketpack-compiler]
    Generated[生成された Rust module]
    Codec[protocol の専用 codec]
    Domain[domain 型]

    Rpf --> Compiler
    Conf --> Compiler
    Compiler --> Generated
    Generated --> Codec
    Codec --> Domain
```

定義を rpf 1 箇所に集めることで、encode 側と decode 側の field 番号の対応を人手で保つ必要がなくなる。
入力は [rpfs](../../daemon/rpfs) と [rocketpack.yaml](../../daemon/rocketpack.yaml)、出力は engine の crate root に組み込む [generated.rs](../../daemon/modules/engine/src/generated.rs) と同名 directory である。
生成物は version 管理し、通常の build で compiler を起動しない。[gen-rocketpack.sh](../../daemon/gen-rocketpack.sh) が core-rs workspace から compiler を実行し、target を `AGENT_TEMP_DIR` 配下に置く。
`rocketpack.yaml` の書式、生成される module の構成、生成物の置き場は [rocketpack-compiler.md](../../daemon/refs/core-rs/docs/design/rocketpack-compiler.md) が正とする。

## 4. wire format の移行

rpf の struct の field と enum の variant は `@N` の番号を持ち、この N が wire 上の field 番号である。
`Option<T>` の field は値が `None` のとき map から省き、要素数もそれに合わせて数える。
未知の番号を受け取った側は、その field を読み飛ばして残りの復号を続ける。
移行は `Option` の省略と未知の field の読み飛ばしを前提にしており、手書き実装と生成物の間で field の有無が違っても復号できる。
長さ制約と decode 時の検査は [core-rs の DESIGN.md](../../daemon/refs/core-rs/docs/DESIGN.md#5-rocketpack) が正とする。

移行の不変条件は wire format を変えないことである。
手書き実装が使っている field 番号をそのまま `@N` へ写し、番号の詰め直しや採番規則の統一を同時に行わない。
1 つの型の宣言と codec を手書きと生成物に分けず、型ごとに正を 1 つにする。
rpf では要素数と byte 長の上限を型に書けるため、移した型では上限を decode の時点で強制できる。
生成 pack が同じ上限を送信時にも検査することを受け入れる。

byte 完全一致の例外は DataMessage の複数要素 map である。
`HashMap` から生成型の `BTreeMap` へ変換すると走査順が変わるが、受信側は map の順序に依存しないため wire 互換として許容する。
この部分は旧から新、新から旧の相互復号と map の内容の同一性で検証し、単一要素 map とその他の通信型は byte 固定を維持する。
NodeProfile の URI は §5.1 の決定に従って受容範囲を変更する。

## 5. 設計判断

### 5.1 決定済み

#### RocketPack 型を rpf から生成する

**決定**
RocketPack で符号化する型は rpf を正とし、Rust の型宣言と `RocketPackStruct` 実装を生成する。
生成物は手で編集せず、型と field 番号の変更は rpf にだけ加える。

**理由**
手書きでは pack と unpack が同じ field 番号を別々の箇所で扱い、対応のずれを型で検出できないためである。
定義を 1 箇所に集めることで、Rust 以外の言語向けの生成先を後から追加する余地も残る。

**却下案**
手書きの `RocketPackStruct` 実装を続ける案は、既存 code に採用の痕跡があるが、型の正が実装の中に散るため採用しない。

#### OmniAddr は string とし、変換を Axus に置く

**決定**
OmniAddr は Axus の rpf では string とし、domain 型との変換は NodeProfile の専用 codec に置く。
OmniHash と OmniCert は omnikit の manifest を dependency として参照する。core-rs は変更しない。

**理由**
現行の NodeProfile が string として符号化している表現を保ち、Axus の通信表現の責務を Axus に置くためである。
変換は `as_str` と `OmniAddr::from` を使い、parse や正規化を加えない。

**却下案**
core-rs に Axus 用の OmniAddr schema を追加する案は、Axus の通信形式の責務を依存先へ持ち込むため採用しない。

#### NodeProfile の URI とネットワークの受容上限を統一する

**決定**
NodeProfileUri を廃止し、URI とネットワークで同じ生成 NodeProfile を使う。
addrs は双方で最大 8 件とし、pack も生成 codec の上限検査に従う。
9 件以上のアドレスを含む既存 URI の互換性は破棄し、decode 失敗を受け入れる。
URI への符号化は `NodeProfile::to_uri` が `Result<String>` を返し、上限超過を呼び出し元で扱う。

**理由**
同じ NodeProfile に二つの schema と受容範囲を持たず、制約の強い方に統一するためである。
`Display` の `fmt::Error` を `to_string` に返すと panic するため、URI 化を専用メソッドとして公開する。

**却下案**
URI 用の無制限 schema と codec を維持する案は、既存 URI の互換性より schema の統一を優先するため採用しない。

#### 必要な型だけ domain 型と通信型を分ける

**決定**
生成型を直接扱える型には手書きの振る舞いを追加する。
domain 型を別に置く場合は専用 codec で明示的に変換し、domain 型から `RocketPackStruct` を外す。

**理由**
全型の二重化を避けつつ、共有所有権、キャッシュ、enum などの Rust 上の表現を保つためである。

**却下案**
domain 型へ薄い `RocketPackStruct` 実装を残す案は、domain 値と通信表現の変換を送受信側で明示する方針に反するため採用しない。

#### DataMessage の map 順序変化と送信時検査を受け入れる

**決定**
DataMessage も生成型へ切り替える。複数要素 map の順序変化は §4 の例外として許容する。
既存の件数上限を schema に写し、上限超過値を生成 pack が送信時にも拒否することを受け入れる。
DataMessage の新たな byte 長上限は、この移行に追加せず次の PR で schema に記述する。

**理由**
受信側の map 解釈と既存の件数上限を維持したまま、codec の正を rpf に集約するためである。

## 6. 現状と残作業

生成型と専用 codec への移行を完了した。
再生成の再現性、旧 codec との互換性、URI の境界と送受信の件数上限を検証した。

DataMessage の新たな byte 長上限は未追加であり、§5.1 の決定に従って次の PR で schema に記述する。
既知の不具合は [issues.md](../issues.md) を参照する。
