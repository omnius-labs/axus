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

Axus は rpf を型定義の正として、Rust の型宣言と `RocketPackStruct` 実装を生成する。
compiler の内部設計は `core-rs` が持ち、本書は Axus から見た入出力と移行規約だけを扱う。

## 3. 生成の入力と出力

```mermaid
flowchart LR
    Rpf[rpf file]
    Conf[rocketpack.yaml]
    Compiler[rocketpack-compiler]
    Generated[生成された Rust module]
    Engine[engine の実装]

    Rpf --> Compiler
    Conf --> Compiler
    Compiler --> Generated
    Generated --> Engine
```

定義を rpf 1 箇所に集めることで、encode 側と decode 側の field 番号の対応を人手で保つ必要がなくなる。
`rocketpack.yaml` の書式、生成される module の構成、生成物の置き場は [rocketpack-compiler.md](../../daemon/refs/core-rs/docs/design/rocketpack-compiler.md) が正とする。

## 4. wire format の移行

rpf の struct の field と enum の variant は `@N` の番号を持ち、この N が wire 上の field 番号である。
`Option<T>` の field は値が `None` のとき map から省き、要素数もそれに合わせて数える。
未知の番号を受け取った側は、その field を読み飛ばして残りの復号を続ける。
移行はこの 2 つの規則を前提にしており、手書き実装と生成物の間で field の有無が違っても復号できる。
長さ制約と decode 時の検査は [core-rs の DESIGN.md](../../daemon/refs/core-rs/docs/DESIGN.md#5-rocketpack) が正とする。

移行の不変条件は wire format を変えないことである。
手書き実装が使っている field 番号をそのまま `@N` へ写し、番号の詰め直しや採番規則の統一を同時に行わない。
1 つの型の宣言と codec を手書きと生成物に分けず、型ごとに正を 1 つにする。
rpf では要素数と byte 長の上限を型に書けるため、移した型では上限を decode の時点で強制できる。

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

### 5.2 保留

#### rpf から参照する core-rs 型の扱い

**現状**
compiler が外部型として参照できるのは dependency に指定した manifest の rpf に定義された型であり、その Rust 上の名前は generator の `dependencies` で対応させる（[rocketpack-compiler.md](../../daemon/refs/core-rs/docs/design/rocketpack-compiler.md)）。
`OmniHash` は core-rs の omnikit の rpf に定義されており、手書き実装も同じ型を `write_struct` で符号化しているため、omnikit の manifest を dependency にすれば参照できる。
`OmniAddr` は rpf に定義がなく、手書きの NodeProfile は string として符号化している。

候補は次の 2 つである。

1. core-rs の rpf に `OmniAddr` を定義して参照すると、型と符号化を 1 箇所に置けるが、core-rs の型の wire format を Axus の都合で決めることになる。
2. Axus の rpf では string などの組み込み型で表すと core-rs を変更せずに済むが、Rust 型との変換が Axus 側に手書きで残る。

**なぜ今決めないか**
現在の wire format を保つ実装はどちらの案でも書けるため、選択は変換を core-rs と Axus のどちらに置くかの問題に留まるためである。

**決める条件**
`OmniAddr` のように rpf に定義のない core-rs 型を含む型を、最初に rpf へ移す前に決める。

## 6. 現状と残作業

engine の型は手書きの `RocketPackStruct` 実装であり、Axus に rpf と `rocketpack.yaml` はない。
