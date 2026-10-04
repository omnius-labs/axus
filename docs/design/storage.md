# 永続化の設計

## 1. このドキュメントについて

本書は node と file の metadata、block 実体、購読の出力 file、状態 directory、schema migration の設計を扱う。
語の定義は [terms.md](../terms.md#2-用語一覧) が正とする。

### 1.1 文書間の責務分担

| 文書または正本 | 受け持つもの |
| --- | --- |
| [design.md](../design.md#11-文書間の責務分担) | 全体構成、他の関心事との境界、横断的な依存関係 |
| 本書 | metadata、block、出力 file の永続化、確定と回収の手順、起動時の回復、状態 directory、migration |
| [file-transfer.md](./file-transfer.md#2-責務と境界) | file の状態遷移、出力先の予約、block の検証、Store と Task の責務 |
| [knowledges.md](../knowledges.md) | RocksDB、SQLite、filesystem の同期と配置に関する外部仕様の根拠 |
| [base/storage](../../daemon/modules/engine/src/base/storage) | block storage の実装 |
| [core/negotiator](../../daemon/modules/engine/src/core/negotiator) | SQLite repository と migration の実装 |

### 1.2 本書の時制について

本文は完成形の設計を記述する。
実装状況と残作業は §6 に集約する。
現在動く範囲を確認する場合は §6 を先に読む。

## 2. 責務と境界

永続化層は、状態遷移と条件検索を必要とする metadata と、hash key で読み書きする block 実体を別の storage model で扱う。
購読の出力は filesystem に置き、購読の行から一時出力と出力先を求めて DB の状態と配置を結び付ける。
file の protocol、探索規則、REST API の contract は持たない。

## 3. metadata と block の分離

| 種別 | 実体 | 保存するもの |
| --- | --- | --- |
| metadata | SQLite と `omnius-core-migration` | 既知 node、file 状態と出力先、block index |
| block | `KeyValueRocksdbStorage` | file 本体と MerkleLayer の byte 列 |
| 購読の出力 | 利用者が指定する filesystem | 一時出力と、確定した出力 file |

metadata は条件検索と状態遷移を必要とするため SQLite に置く。
block は hash key による読み書きが中心であり、大きな byte 列を metadata query から分離するため RocksDB に置く。

### 3.1 `state_dir` の構造

永続状態は component ごとに directory を分ける。

```text
<state_dir>/
├── identity/
├── repo/
├── file_publisher/
│   ├── repo/
│   └── blocks/
└── file_subscriber/
    ├── repo/
    └── blocks/
```

publisher と subscriber の保存領域を分けることで、一方の再構築や削除が他方の状態を直接壊さない。
一時出力は rename で出力先へ配置するため、出力先の親 directory に作り、`state_dir` には置かない。
`identity/` は node の署名鍵を持ち、消すと次の起動で別の鍵と node ID が作られるため、ほかの directory と違って作り直しでは元に戻らない。
directory 名の追加や移行が必要な場合は migration と rollback の単位を明示する。

## 4. 論理 key と migration

### 4.1 論理 key

[KeyValueRocksdbStorage](../../daemon/modules/engine/src/base/storage/key_value_rocksdb_storage.rs) は、論理 key から内部 ID を引く `names` と、内部 ID から実体を引く `blocks` および `metas` を分ける。
公開処理の commit は論理 key を uncommitted から committed へ変更し、同じ block 実体を再利用する。

1 回の commit で行う複数 block の rename は、1 つの RocksDB transaction にまとめるため、一部の block だけが移った状態は残らない。
この原子性は対象が同じ RocksDB database 内にあることを前提とし、別 database への移動には成り立たない。
rename と SQLite の commit を合わせた原子性はなく、その間の失敗は §4.3 の sweep で回収する。

### 4.2 永続化の保証と境界

保存・確定の成功後は、process の終了、OS クラッシュ、電源断を経ても、それを示す metadata と実体を保持する。
対象は Linux、macOS、Windows であり、保証は OS、filesystem、device が同期要求と書き込み順序を守ることを前提とする。
device の故障や、利用者による確定済み出力の変更・削除は、この保存保証に含めない。
同期の失敗は操作の失敗として扱い、保証を弱めた成功を返さない。

保存・確定の成功を返すには、非同期 I/O の完了や `flush` に加えて、次の同期が成功している必要がある。

| 保存先 | 成功を確定する条件 |
| --- | --- |
| RocksDB | WAL を有効にし、`sync=true` の書き込みで、block の追加・論理 key の変更・削除を永続化する。macOS では RocksDB を `HAVE_FULLFSYNC` を定義して build し、同期に `F_FULLFSYNC` を使わせる |
| SQLite | 各 connection に WAL と `synchronous=FULL` を明示する。macOS では `fullfsync=ON` も指定する |
| 出力 file | 復号 writer の `flush` の後、`sync_all` で内容を永続化する |
| 出力先の directory entry | Linux と macOS は配置後に親 directory を `sync_all` する。Windows は配置に `MOVEFILE_WRITE_THROUGH` を指定する |

実体を先に永続化し、それを示す metadata を後から確定する。
この順序が崩れると、確定した metadata が電源断で失われた実体を参照する。
一時出力の作成と削除の entry は同期しない。作成した entry を失った場合は §4.5 の回復で Failed になる。
削除した entry が電源断で戻り、その購読の行がすでにない場合は、一時出力が出力先の directory に残る。sweep は行から一時出力の名前を求めるため、この file を回収しない。
外部仕様は [RocksDB の同期](../knowledges/rocksdb-write-durability.md)、[SQLite の同期](../knowledges/sqlite-wal-durability.md)、[file と directory の同期](../knowledges/filesystem-sync-durability.md) を参照する。

### 4.3 公開の commit と回収

Store は次の処理を publisher の mutex 内で行う。符号化はその外で行う。

1. Processing の行と、同じ root hash の committed file の有無を確認する。
2. committed file がなければ、その root hash に残る committed key を消し、重複 hash を除いた block の論理 key を uncommitted から committed へ移す。削除とすべての rename を 1 つの RocksDB transaction にまとめて同期する。committed file があればその実体を共有し、変更しない。
3. SQLite の 1 transaction で committed file と block index を登録し、uncommitted file の行を消す。この transaction の永続化後に commit 成功を返す。

2 と 3 の間で失敗すると、どの committed file からも参照されない committed key が残り得る。同じ root hash の次の commit は、手順 2 でこの key を消してから移すため衝突しない。
この key と、行を持たない一時 ID の uncommitted key は、sweep が回収する。
回収対象を事前に記録しない。削除してよいかは、削除する時点の SQLite の参照だけで判定する。

sweep は mutex 内で RocksDB の key と SQLite の行を照合し、参照のない key を削除して同期する。

| key | 削除する条件 |
| --- | --- |
| uncommitted | その一時 ID の Pending または Processing の行がない |
| committed | その root hash の committed file がない |

SQLite の commit の結果が不明なときも、sweep は削除の時点で SQLite を読み直すため、確定していた committed file の block を消さない。
SQLite を読めなければ sweep は何も消さずに失敗する。

sweep は Store の open で行い、稼働中は commit の失敗と、cancel・remove での実体の削除の失敗の後に行う。
TaskEncoder は、sweep が成功するまで間隔を延ばしながら再実行する。sweep が必要かどうかは memory 上にだけ持ち、再起動時は open の sweep が引き継ぐ。
sweep は key 全体を走査するため、所要時間は保存した block の数に比例する。

### 4.4 購読 block の保存と参照の削除

受信 block は RocksDB に書いて永続化した後、SQLite に downloaded と進捗を永続化する。
同期をまとめる場合も、その batch 全体を永続化する前にいずれの downloaded も確定しない。
実体だけが残った場合は再受信または再登録できるが、downloaded の実体がない状態を正常な回復結果として扱わない。

remove は最後の参照を SQLite から消した後、Store の write lock 内で root の実体を消す。
実体の削除に失敗した root は、購読から参照されない root として sweep が回収する。
subscriber の sweep は write lock 内で購読の行を読み直し、参照のある root の実体を消さない。
publisher の remove も同じ順序で行い、参照確認には publisher の mutex を使う。

### 4.5 出力の確定と回復

出力先は購読の行に、解決した親 directory の絶対 path と最終 entry 名の組として記録する。
Downloading、Decoding、Finalizing の行に限る部分一意 index を置き、同じ出力先の進行中の購読を登録時に拒否する。
状態が Completed、Failed、Canceled に変わるか行が消えると、同じ transaction で一意制約の対象から外れる。
名前は表記どおりに比較するため、大小文字や Unicode 正規化だけが違う別表記は登録時に拒否されず、確定時の配置で検出する。
既存 entry の確認は symlink を辿らず、dangling symlink も衝突として扱う。

**一時出力の名前は、出力先と同じ directory の `.<entry 名>.axus-<購読 ID>.part` とし、daemon がこの名前の file を所有する。**
所有の判定はこの名前だけで行い、file の識別情報を記録しない。
この規約は、外部 process がこの名前の file を作成・削除・rename しないことを前提とする。
前提が崩れると、再起動時に配置済みかどうかを誤判定し、配置済みの購読を Failed にするか、外部の file を一時出力として扱う。

確定の順序を次に固定する。

1. Store は一時出力の名前に既存の file があれば消し、新しく作成して Task に writer を渡す。名前が長すぎるなどで作成できなければ Failed にする。Task は一時出力だけへ復号する。
2. Store は writer の `flush` の後、一時出力を `sync_all` する。
3. SQLite の 1 transaction で Decoding を Finalizing に条件付き更新する。cancel/remove が先に確定していれば配置せず、一時出力を消す。ここが確定開始である。
4. 一時出力を、既存 entry を置き換えない rename で出力先へ配置し、§4.2 の directory entry の同期を行う。
5. SQLite に Completed を永続化する。成功はこの後に返す。

置き換えない rename は、Linux では `renameat2` の `RENAME_NOREPLACE`、macOS では `renamex_np` の `RENAME_EXCL`、Windows では `MOVEFILE_REPLACE_EXISTING` を指定しない `MoveFileExW` を使う。
`std::fs::rename` は既存の出力先を置き換えるため使わない。
出力先に entry があって rename が失敗したときと、保存先がこの rename に対応していないときは、Failed にして一時出力を消す。
一時出力がないために rename が失敗したときは、再起動時の「一時出力も出力先もない」と同じく扱い、出力先があれば Completed、なければ Failed にする。
そのほかの I/O 障害では Finalizing を保ち、TaskDecoder が間隔を延ばしながら Store に配置を再試行させる。
外部仕様は [置換しない rename](../knowledges/filesystem-output-publication.md) を参照する。

rename は一時出力の名前を消して出力先の名前を作る操作を 1 回で行うため、再起動時は 2 つの名前の有無から配置が済んだかを判定できる。

| 再起動時の状態 | 処理 |
| --- | --- |
| 一時出力があり、出力先がない | 配置前に終了した。手順 4 から再試行する |
| 一時出力があり、出力先もある | 開始後に外部 process が出力先を作った。Failed にして一時出力を消し、出力先は残す |
| 一時出力がなく、出力先がある | 配置後に終了した。親 directory を同期してから Completed を記録する |
| 一時出力も出力先もない | 一時出力を失った。Failed にする。block は残るため、再び subscribe すれば復号し直せる |
| 親 directory に届かない、または I/O 障害で判定できない | Finalizing を保ち、稼働中に TaskDecoder が判定を再試行させる |

最後の行の例は、出力先の disk を外したまま再起動した場合である。
この購読だけが Finalizing に留まり、ほかの購読と Store の open は止めない。

確定前の cancel は、先に Downloading または Decoding から Canceled への条件付き更新を行い、その後で Task の writer の停止を待って一時出力を消す。
更新が先に確定するため、Task は Finalizing へ進めず、削除した一時出力を配置しようとすることはない。
一時出力の削除に失敗しても Canceled は戻さず、残った一時出力は sweep が消す。
確定前の remove は、同じ条件付き更新で Canceled にしてから一時出力を消し、その後に行を消す。削除に失敗したら Canceled の行を残して remove を失敗させる。行が残っている限り、一時出力の名前を行から求められる。
subscriber の sweep は、参照されない root の実体に加えて、Canceled と Failed の行が指す一時出力を消す。

### 4.6 起動時の回復

Store は回復を終えてから Task と API を受け付ける。
`state_dir` の SQLite または RocksDB を読み書きできない場合は open を失敗させ、曖昧な状態で新しい操作を受け付けない。
出力先の directory に起因する判定不能は §4.5 のとおり購読ごとに保留し、open を失敗させない。

| Store | 起動時の回復 |
| --- | --- |
| FilePublisherStore | すべての uncommitted key を消し、Processing を Pending に戻した後、sweep を行う。open の中では符号化が動いていないため、Pending の行の key も消してよい |
| FileSubscriberStore | Finalizing を §4.5 の規則で回復する。Downloading と Decoding は保存済み block から処理を再開し、rank 0 の復号では §4.5 の手順 1 で一時出力を作り直す。その後 sweep を行う |

各操作は永続化済みの情報から何度でも再実行できる。
処理の順序と同期の両方により、存在を確定した metadata が未永続化の実体を参照することを防ぐ。

### 4.7 migration

[repo.rs（公開側）](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/repo.rs)、[repo.rs（購読側）](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/repo.rs)、[node_finder_repo.rs](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs) は、名前付きの MigrationRequest を順に適用する。
適用済みの名前を持つ migration は再実行しない。

配布済みの状態へ適用され得る migration は書き換えず、新しい名前の migration で前進させる。
初回 release 前の migration を直接修正する場合でも、適用済み状態が存在しないことを確認し、その確認結果を変更記録に残す。
Finalizing と出力先の一意制約の追加では、未稼働の初期 schema を変更し、旧状態を移行する互換処理は設けない。

## 5. 設計判断

### 5.1 決定済み

#### metadata と block 実体を別の storage に置く

**決定**
検索と状態遷移を伴う metadata は SQLite に置き、hash key で扱う block 実体は RocksDB に置く。

**理由**
関係 query と大きな byte 列の読み書きを同じ storage model に押し込めず、それぞれの access pattern に合わせるためである。

#### OS クラッシュと電源断までを保存保証に含める

**決定**
実体の永続化を先に完了し、それを示す metadata を後から永続化する。
同期には各 storage の同期設定と、std の `sync_all` および OS ごとの置換しない rename だけを使う。
出力 file は配置した directory entry も同期してから Completed を記録する。

**理由**
process の正常終了を保存の成立条件にせず、完了を返した file と進捗を障害後にも保持するためである。
書き込み順序だけでは OS の buffer と device cache に残る実体を保護できない。
同期の手段を標準の呼び出しに限ると、保存先ごとの機能判定を持たずに、同期の失敗を操作の失敗として返すだけで済む。

**却下案**
process の終了だけを保証する案は同期書き込みを減らせるが、完了を記録した後の電源断で実体を失い得るため採らない。
保存先が必要な同期に対応するかを事前に判定し、対応しない保存先を拒否する案は、OS と filesystem ごとの判定処理が要るため採らない。

#### 一時出力を名前で所有し、置換しない rename で確定する

**決定**
一時出力は購読 ID を含む規約の名前で作り、その名前だけで daemon の所有と判定する。
確定は既存 entry を置き換えない rename で行い、再起動時は一時出力と出力先の有無から配置の有無を判定する。

**理由**
rename は 2 つの名前の付け替えを 1 回で行うため、file の識別情報を記録せずに配置済みかを判定できる。
一時出力の名前を購読の行から求められるので、回収のための記録を別に持たずに済む。

**却下案**
一時出力の識別情報を記録し、出力先へ hard link を追加して Completed まで一時側の link を残す案は、所有の照合に OS ごとの file identity が要り、識別情報を記録する前に終了したときの所有不明の file も扱う必要がある。
存在を確認してから `std::fs::rename` で配置する案は、確認と配置の間に作られた外部の file を置き換える。

#### 参照のない実体を照合で回収する

**決定**
参照のない block の key と一時出力は、削除の時点で SQLite の参照と照合する sweep で回収する。
回収対象を事前にも失敗後にも記録しない。

**理由**
削除の可否を現在の参照だけで判定すると、失敗した操作の種類ごとに回収情報を書く必要がない。
1 回の commit の rename を 1 つの RocksDB transaction にまとめるため、回収対象を事前に記録しなくても、残り得るのは参照のない key だけになる。

**却下案**
block の移動前に commit の対象を記録し、失敗分を永続的に記録して再試行する案は、記録の書き込み順序、記録用の table、回収完了までの再 import の拒否が要る。

### 5.2 保留

この関心事に閉じる保留はない。

## 6. 現状と残作業

NodeFinder、publisher、subscriber の repository と block storage がある。
publisher と subscriber は Store の open の中で、次に示す block の回収を終えてから、Task と入口の操作を受け付ける。
publisher は uncommitted の key と参照のない committed の key を消し、Processing を Pending に戻す。
subscriber は購読から参照されない root hash の key を消す。

§4.2 の同期条件、§4.3 の 1 transaction での rename と稼働中の sweep、§4.5 の一時出力、出力先の一意制約、置換しない rename による確定と回復は未実装である。
現状の起動時回復は block の orphan 回収までである。
これらの手順の検証には、状態遷移の境界ごとの強制終了と、Linux、macOS、Windows での同期条件の確認が必要である。
macOS の RocksDB に `HAVE_FULLFSYNC` を与える build 設定もなく、設定が RocksDB の build に反映されることの確認も残っている。
確認済みの不具合は [issues.md](../issues.md) を参照する。
