# Rust コーディング規約

## 1. 本書の役割

本書は、Axus の手書き Rust コードで責務をどこへ置くかを定める。
想定読者は、このリポジトリの Rust を変更する開発者と AI エージェントである。

rustfmt が整形を、clippy が一般的な誤りを検査する。
本書は、それらが判定しない型、関数、module の責務境界だけを扱う。

本書は omnius-labs の Rust リポジトリが共有する規約であり、正本は core-rs の `docs/coding/rust.md` である。
2 章から 4 章は pxna と axus でも同一に保ち、変更する場合は 3 つのリポジトリを同時に更新する。
1 章と 5 章は、適用範囲と検査コマンドがリポジトリごとに異なる。

### 1.1 文書間の責務分担

| 文書                      | 受け持つもの                                          |
| ------------------------- | ----------------------------------------------------- |
| 本書                      | Rust における責務の置き場所、自由関数の例外、検査方法 |
| [design.md](../design.md#11-文書間の責務分担) | 機能の設計、不変条件、採用理由                        |
| [issues.md](../issues.md) | コードで確認した明確な不具合                          |

型が何を担当するかは設計文書が定める。
本書は、その責務を Rust の型、関数、module へどう配置するかを定める。

### 1.2 適用範囲

規約と `cargo make lint-style` の対象は、`daemon/lint-style.toml` が定める。
Axus では、次の手書き crate を対象とする。

- `daemon/modules/engine`
- `daemon/entrypoints/daemon`

次のコードは対象外とする。

| 対象外                          | 理由                                    |
| ------------------------------- | --------------------------------------- |
| `daemon/entrypoints/interface`  | OpenAPI Generator の生成物である        |
| `daemon/refs`                   | 別リポジトリの参照である                |
| `target` と `.git`              | build 生成物と version 管理の内部である |
| `#[cfg(test)]` 配下と test 関数 | テストの構造は本書で制約しない          |
| `fn main`                       | 言語が要求する entrypoint である        |

## 2. 振る舞いの所有者

### 2.1 自然な所有者を優先する

入力や結果を表す型、状態を持つ型、外部形式を解釈する adapter がある処理は、その型の `impl` に置く。
呼び出し元が関数名と引数だけから状態と入力を見分ける必要がなくなり、変更の影響範囲も型へ閉じるためである。

```rust
// 避ける形
fn detect_format(head: &[u8]) -> Result<ArchiveFormat> { ... }

// 所有者が明確な形
impl ArchiveFormat {
    fn detect(head: &[u8]) -> Result<Self> { ... }
}
```

自由関数を型へ移すためだけに、意味のない型を作る必要はない。
次の処理は、自然な所有者を持たない場合に自由関数としてよい。

- 複数の対等な型から使う、状態を持たない低レベルのアルゴリズム
- OS API や FFI を包む platform 境界
- framework が関数を要求する callback

本番コードへ自由関数を置く場合は、4 章の marker で理由をコードの隣に残す。

### 2.2 unit struct と self なし関連関数

フィールドを持たない型は、型名が一つの役割を表す場合に使える。
stateless な trait 実装、parser、migrator、converter がこの形に該当する。

```rust
struct ElementParser;

impl ElementParser {
    fn parse_tcp_ip(element: &Element) -> Result<SocketAddr> { ... }
}
```

unit struct を自由関数の退避先として使うわけではない。
`Util`、`Helpers`、`Functions` のように、どの処理でも入れられる名前の型は作らない。

self を取らない関連関数は、`Self` を返す constructor だけに限定しない。
その型が表す役割に操作群が収まり、別の責務を追加する場所になっていなければよい。

### 2.3 一つのファイルは一つの概念を扱う

ファイルは、型の個数ではなく責務のまとまりで分ける。
主型、その trait 実装、専用の入力型や出力型、fake は同居させてよい。

`clock.rs` に実装と fake を置く形や、`timestamp.rs` に同じ表現の複数精度を置く形は、一つの概念に収まる。
一方、`util.rs` や `helpers.rs` に関係の薄い処理を集めたり、`extract.rs` のように処理段階だけを名前にして関数を置いたりしない。

ファイル名から、そのファイルが所有する概念を説明できる状態を保つ。

## 3. 型が保持するもの

### 3.1 依存は寿命で分ける

構築後も同じ相手を使い続ける場合、その依存を constructor で受け取り、フィールドに保持する。
メソッド引数には、呼び出すたびに変わる操作入力だけを置く。

generic、trait object、`Arc` のどれを使うかは、共有の要否と動的 dispatch の要否で決める。
外部境界だからという理由だけで、すべてを `Arc<dyn Trait>` に統一しない。

引数の個数だけでは依存と入力を区別できない。
引数が増えた場合は、個数を減らす前に各値の寿命を確認する。

### 3.2 エラー変換は文脈の有無で分ける

外部エラーから crate のエラーへの一対一で文脈に依存しない変換は、`From` に置く。
どこで `?` を使っても同じ `ErrorKind` になる変換だけが該当する。

暗号化、未対応形式、破損のように、呼び出した機能の文脈で分類が変わる変換は、外部形式を所有する adapter や domain type に置く。
この変換では、可能な限り元の error を source として保持する。

### 3.3 constructor の名前

型を作る標準経路が一つなら、失敗して `Result<Self>` を返す場合も `new` を使える。
`parse`、`open`、`connect`、`acquire` は、単なる構築ではなく操作の意味を名前に出す場合に使う。

`Default` は、有効な既定値が一つに定まる型だけに実装する。
必須値を仮の値で埋めるためには使わない。

## 4. 自由関数の marker

### 4.1 恒久的な例外

自然な所有者を持たない本番自由関数には、宣言の直前に `allow` marker を置く。
理由には、なぜ型の `impl` に属さないのかを書く。

```rust
// omnius-lint:allow(free-fn) OS ごとの system call を cfg module の境界として公開する
fn process_exists(pid: u32) -> bool { ... }
```

### 4.2 未解消の責務配置

自然な所有者があるものの、挙動変更を伴う移動を別の変更へ分ける場合は `debt` marker を置く。
理由には、移動先となる型または欠けている概念と、今回移動しない理由を書く。

```rust
// omnius-lint:debt(free-fn) ArchiveExtractor へ移すリファクタを挙動変更と分離する
fn materialize(...) -> Result<MaterializedFile> { ... }
```

`debt` は許可ではなく、現在残っている責務配置の負債である。
新しい変更で `debt` を追加せず、既存の関数を移動または削除したときは marker も削除する。
この増減は PR の差分レビューで確認する。

### 4.3 marker の検査

marker の理由は必須である。
未マークの自由関数、不正な marker、空の理由、対応する関数がない marker は `cargo make lint-style` を失敗させる。

検査器は `debt` を表示するが、既存の挙動を保ったまま規約を導入できるよう、`debt` だけでは失敗させない。

unit struct、self なし関連関数、ファイルの概念境界、依存の寿命、エラー変換は意味判断を要するため、レビューで確認する。

検査器の実装は core-rs の `entrypoints/lint-style` にあり、pxna と axus は submodule 経由で同じ bin を使う。

## 5. 検査コマンド

```sh
cd daemon
cargo make lint-style
cargo make lint
cargo make test
```

`cargo make lint-style` は本書の自由関数 marker を検査する。
`cargo make lint` は rustfmt、clippy、`lint-style` を実行する。
`cargo make test` は lint の後に全 feature のテストを実行する。

GitHub Actions は `cargo make lint` と `cargo make test` を入口として使うため、同じ検査が CI でも実行される。
