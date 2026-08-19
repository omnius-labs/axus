# Axus 既知の不具合

本書は、コードを読んで確認した明確な不具合を扱う。
設計上の未決事項は扱わず、[DESIGN.md §13.2](./DESIGN.md#132-保留) に置く。

## この文書の使い方

- GitHub Issue に起票したら、一覧表の Issue 列に番号を追記する。
- 修正されたら、一覧表の行と項目本体を削除する。
- 行番号はコード変更でずれるため、修正時に該当箇所を再確認する。

調査日: 2026-07-26
対象コミット: `ac38557`

## 一覧

| ID            | 概要                                                          | 深刻度 | Issue  |
| ------------- | ------------------------------------------------------------- | ------ | ------ |
| [I-1](#i-1)   | `DaemonState` が `AxusService` を生成せず、P2P 層が起動しない | 高     | 未起票 |
| [I-2](#i-2)   | ファイル関連の SQLite schema が初期化時にエラーになる         | 高     | 未起票 |
| [I-3](#i-3)   | SQL が存在しない列 `property` を参照している                  | 高     | 未起票 |
| [I-4](#i-4)   | `MerkleLayer.rank` の解釈が encoder と decoder で 1 ずれる    | 高     | 未起票 |
| [I-5](#i-5)   | FileExchanger 要求で受理 task が panic し、受信が停止する     | 高     | 未起票 |
| [I-6](#i-6)   | version 交渉が積集合ではなく和集合になっている                | 中     | 未起票 |
| [I-7](#i-7)   | 署名鍵の識別子が `"TODO"` 固定である                          | 中     | 未起票 |

I-1 は P2P component を daemon から実行するための前提である。
I-2、I-3、I-4 はファイル公開と購読の同じ経路にあるため、結線前にまとめて修正する。

<a id="i-1"></a>
## I-1. `DaemonState` が `AxusService` を生成せず、P2P 層が起動しない

**深刻度: 高**

### 症状

daemon を起動しても P2P 通信が行われない。
HTTP server だけが待ち受ける。

### 該当箇所

- [state.rs:24](../daemon/entrypoints/daemon/src/state.rs#L24)：`DaemonState::new` は `conf` と `temp_dir` だけを保持して返す。
- [executor.rs:100](../daemon/entrypoints/daemon/src/executor.rs#L100)：`state.engine.shutdown()` はコメントアウトされている。

### 原因

`DaemonState` が `AxusService` を生成して保持する処理がない。

### 影響

daemon の通常の起動経路から P2P component を利用できない。
I-2 から I-7 の経路を daemon として実行する前提も成立しない。

### 対応方針

`DaemonState` に `AxusService` を保持させる。
`Executor::handle_start` の終了時に `AxusService::shutdown()` を呼ぶ。
HTTP API と P2P の待ち受けアドレスは [DESIGN.md §5.2](./DESIGN.md#52-設定の境界) に従って分離する。
FileExchanger の結線は I-2、I-3、I-4 の修正後に行う。

<a id="i-2"></a>
## I-2. ファイル関連の SQLite schema が初期化時にエラーになる

**深刻度: 高**（I-1 により現時点では未顕在）

### 症状

`FilePublisherRepo::new` と `FileSubscriberRepo::new` が migration の適用時にエラーを返す。
そのため `FilePublisher` と `FileSubscriber` を構築できない。

### 該当箇所

publisher 側の索引定義は [publisher_repo.rs:82](../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L82) にある。

```sql
CREATE INDEX IF NOT EXISTS index_file_id_rank_index_for_uncommitted_blocks
    ON committed_blocks (file_id, rank ASC, `index` ASC);
```

索引名は `uncommitted_blocks` 用だが、対象 table は `committed_blocks` である。
`committed_blocks` は `root_hash` を持つ一方で、`file_id` を持たない。

subscriber 側の `files` table は [subscriber_repo.rs:41](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L41) にある。

1. [subscriber_repo.rs:42](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L42) と [subscriber_repo.rs:54](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L54) が primary key を二重に宣言している。
2. [subscriber_repo.rs:54](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L54) が table に存在しない `file_name` を参照している。
3. [subscriber_repo.rs:45](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L45) は `depth` を定義するが、[SubscribedFile:6](../daemon/modules/engine/src/core/negotiator/file/model/subscribed_file.rs#L6) の対応 field は `rank` である。
4. [subscriber_repo.rs:64](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L64) の索引は `is_downloaded` を参照するが、`blocks` table の列名は `downloaded` である。

### 原因

migration の SQL と、対象 table および model の名前が一致していない。

### 影響

ファイル公開と購読の repository を初期化できない。
I-1 により FileExchanger が daemon から構築されないため、現在の通常起動では顕在化しない。

### 対応方針

対象 table と model に合わせて索引、primary key、列名を修正する。
既存 MigrationRequest を直接修正できるかは、[DESIGN.md §10.4](./DESIGN.md#104-schema-migration) に従って判断する。
適用済み環境が存在しないことを確認できた場合に限り、既存 MigrationRequest を直接修正する。
適用済み環境が存在する場合、または存在しないことを確認できない場合は、新しい MigrationRequest を追加する。
修正後は publisher と subscriber の初期化 test を追加する。

<a id="i-3"></a>
## I-3. SQL が存在しない列 `property` を参照している

**深刻度: 高**（I-1 と I-2 により現時点では未顕在）

### 症状

ファイル情報を取得または挿入する query が `no such column: property` で失敗する。

### 該当箇所

どの対象 table にも `property` 列はない。

| ファイル             | 該当行                                                                                                                                                                                                                                                                                                                                                                                               | 対応する列 |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| `publisher_repo.rs`  | [111](../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L111)、[125](../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L125)                                                                                                                                                                                                       | `attrs`    |
| `publisher_repo.rs`  | [301](../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L301)、[330](../daemon/modules/engine/src/core/negotiator/file/file_publisher/publisher_repo.rs#L330)                                                                                                                                                                                                       | `attrs`    |
| `subscriber_repo.rs` | [91](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L91)、[106](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L106)、[121](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L121)、[138](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L138) | `priority` |

subscriber 側では `attrs` が別に選択されているため、`property` は `priority` の位置にある。
publisher 側では `property` が `attrs` の位置にある。
[subscriber_repo.rs:308](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/subscriber_repo.rs#L308) の `INSERT` は `attrs` と `priority` を使っている。

### 原因

query の列名が table schema および model と一致していない。

### 影響

I-2 を修正して repository を初期化できても、対象 query は失敗する。
ファイル公開と購読の処理を継続できない。

### 対応方針

publisher 側の `property` を `attrs` に置換する。
subscriber 側の `property` を `priority` に置換する。
I-2 の `depth` と `rank` の不一致も同時に修正する。
再発防止として、`sqlx::query!` による compile 時検証を導入できるか検討する。

<a id="i-4"></a>
## I-4. `MerkleLayer.rank` の解釈が encoder と decoder で 1 ずれる

**深刻度: 高**（購読開始の入口がなく現時点では未顕在）

### 症状

ファイルの復号が最初の Merkle layer で `InvalidFormat` を返す。

### 該当箇所

encoder は [task_encoder.rs:228](../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L228) で、格納先 block と同じ rank を記録する。

```rust
let mut rank = 1;
loop {
    let merkle_layer = MerkleLayer {
        rank,
        hashes: std::mem::take(&mut current_block_hashes),
    };
```

decoder は [task_decoder.rs:203](../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L203) で、格納先 block より 1 小さい rank を期待する。

```rust
if merkle_layer.rank != (file.rank - 1) {
    return Err(Error::new(ErrorKind::InvalidFormat));
}
```

### 原因

`MerkleLayer.rank` が格納先 block と子 block のどちらの rank を表すかが確定していない。
encoder と decoder は異なる意味で実装されている。

### 影響

購読側の復号が成立しない。
購読開始の入口がないため、現在の daemon API からは顕在化しない。

### 対応方針

[DESIGN.md §13.2](./DESIGN.md#merklelayerrank-の意味) で rank の意味を決める。
決定した意味に合わせて encoder と decoder を同時に修正する。
encode から decode までの round-trip test で契約を固定する。

<a id="i-5"></a>
## I-5. FileExchanger 要求で受理 task が panic し、受信が停止する

**深刻度: 高**（通常の daemon では I-1 により未顕在、AxusService 起動時は外部から誘発可能）

### 症状

`AxusService` を起動したノードへ `SessionType::FileExchanger` の接続要求を 3 回送ると、着信を処理する task がすべて停止する。
process は生存するが、それ以降の着信を受理できない。

### 該当箇所

[accepter.rs:50](../daemon/modules/engine/src/core/session/accepter.rs#L50) は `NodeFinder` 用の受理 queue だけを作る。

```rust
for typ in [SessionType::NodeFinder].iter() {
    let (tx, rx) = mpsc::channel(20);
}
```

[accepter.rs:194](../daemon/modules/engine/src/core/session/accepter.rs#L194) は request された種別の queue を `unwrap()` で取り出す。

```rust
if let Ok(permit) = self.senders.lock().await.get(&typ).unwrap().try_reserve() {
```

`typ` が `FileExchanger` の場合、`get()` は `None` を返す。
その結果、`unwrap()` が panic する。

### 原因

SessionType の追加と受理 queue の初期化対象が同期していない。
外部 request から決まる値に対して `unwrap()` を使っている。

### 影響

`TaskAccepter` は 3 本あり、1 回の panic で 1 本ずつ停止する。
3 回の要求で着信用 task がすべて停止する。
[AxusService::create_node_finder](../daemon/modules/engine/src/service.rs#L41) は SessionAccepter を構築するため、AxusService を起動する経路では外部から誘発できる。
通常の daemon は I-1 により AxusService を構築しないため、現在の通常起動では未顕在である。

### 対応方針

1. 受理 queue をすべての SessionType に対して初期化する。
2. `unwrap()` を除去し、queue がない種別には `Reject` を返す。
3. SessionType の追加漏れを compile 時に検出できるよう、全 variant を列挙して初期化する。
4. FileExchanger request を繰り返しても task が停止しないことを test する。

<a id="i-6"></a>
## I-6. version 交渉が積集合ではなく和集合になっている

**深刻度: 中**（V1 しか存在しないため現時点では未顕在）

### 症状

対応 version が複数になった場合、双方に共通しない version でも交渉が成立する。
その結果、互いに解釈できない message を交換する。

### 該当箇所

次の 3 箇所が bit flag の和集合を取っている。

- [connector.rs:36](../daemon/modules/engine/src/core/session/connector.rs#L36)
- [accepter.rs:169](../daemon/modules/engine/src/core/session/accepter.rs#L169)
- [task_communicator.rs:149](../daemon/modules/engine/src/core/negotiator/node/task_communicator.rs#L149)

```rust
let version = send_hello_message.version | received_hello_message.version;
```

### 原因

双方が対応する version の積集合ではなく、どちらかが対応する version の和集合を計算している。

### 影響

V1 だけが定義されている間は結果が変わらない。
V2 以降を追加すると、接続成立後に message の解釈が食い違う。

### 対応方針

3 箇所の `|` を `&` に変更する。
積集合が空の場合は、対応 version がないことを示す error を返す。
複数の共通 version から 1 つを選ぶ規則は [DESIGN.md §13.2](./DESIGN.md#session-の-version-選択) で決める。
共通 version がない場合と複数ある場合の test を追加する。

<a id="i-7"></a>
## I-7. 署名鍵の識別子が `"TODO"` 固定である

**深刻度: 中**（設計論点に依存）

### 症状

すべての AxusService が同じ識別子から署名鍵を生成する。

### 該当箇所

[service.rs:56](../daemon/modules/engine/src/service.rs#L56) が固定文字列を OmniSigner に渡している。

```rust
let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "TODO")?);
```

### 原因

node identity の設計が決まる前の仮値が残っている。

### 影響

session 層の相互認証で node ごとの鍵を識別できない。
[node_finder.rs:70](../daemon/modules/engine/src/core/negotiator/node/node_finder.rs#L70) が生成する `NodeProfile.id` も署名鍵と結び付いていない。

### 対応方針

[DESIGN.md §13.2](./DESIGN.md#nodeprofileid-と署名鍵の結合) で node identity と署名鍵の関係を決める。
決定した identity に基づいて signer の識別子と永続化方法を実装する。
node ごとに異なる鍵が生成または復元されることを test する。

## ここに含めていないもの

次の項目は、確認済みの不具合ではなく、設計判断または新規実装を必要とする。
対応する設計は DESIGN.md 側で扱う。

| 項目                                        | 参照                                                         |
| ------------------------------------------- | ------------------------------------------------------------ |
| session の secure channel がない            | [DESIGN.md §13.2](./DESIGN.md#session-の-secure-channel)     |
| FileExchanger の block 交換 protocol がない | [DESIGN.md §9.4](./DESIGN.md#94-接続相手と-block-の検証)     |
| 購読開始の入口がない                        | [DESIGN.md §14.1](./DESIGN.md#141-現状)                      |
| NodeFinder の能動探索がない                 | [DESIGN.md §13.2](./DESIGN.md#nodefinder-の能動探索と冗長度) |
| Web of Trust が未設計である                 | [DESIGN.md §13.2](./DESIGN.md#web-of-trust-の単位と伝播)     |
| Profile と memo が未実装である              | [DESIGN.md §12.2](./DESIGN.md#122-層ごとの責務)              |
| 長時間 REST 操作の表現が未決である          | [DESIGN.md §13.2](./DESIGN.md#長時間-rest-操作の表現)        |
