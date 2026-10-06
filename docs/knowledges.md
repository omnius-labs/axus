# Axus の外部知識

本書は依存ライブラリと OS の挙動について、一次資料で確認した事実を扱う。
Axus の設計判断と実装状況は設計文書、コードの不具合は issues が持つ。

## 1. この文書について

### 1.1 文書間の責務分担

| 文書 | 受け持つもの |
| --- | --- |
| 本書 | 外部の事実の一覧、検証対象、出所、確認日、項目間の関係 |
| [design.md](./design.md#11-文書間の責務分担) | 文書族の地図と、外部仕様を前提にした設計判断 |
| [terms.md](./terms.md#2-用語一覧) | プロジェクトの語の定義と表記 |
| [issues.md](./issues.md) | 外部仕様との照合で判明した実装の不具合 |

### 1.2 使い方

依存 version、対応 OS、外部仕様が変わったら検証対象を突き合わせ、事実と根拠を更新する。
過去の版の記述を本文に残さない。
「文献のみ」は参照日から 3 か月ごとに一次資料を読み直し、内容が同じなら確認日を進める。
障害試験を行っていない項目を「検証済み」と扱わない。

## 2. 一覧

| 項目 | 検証対象 | 出所 | 最終確認日 |
| --- | --- | --- | --- |
| [RocksDB の電源断耐久性は sync 指定と build 時の定義に依存する](./knowledges/rocksdb-write-durability.md) | rocksdb 0.24.0 / librocksdb-sys 0.17.3+10.4.2 / cc 1.2.56 | 検証済み | 2026-10-06 |
| [SQLite の WAL の電源断耐久性は同期条件に依存する](./knowledges/sqlite-wal-durability.md) | SQLx 0.9.0 / libsqlite3-sys 0.30.1 / bundled SQLite 3.46.0 | 文献のみ | 2026-10-04 |
| [file の I/O 完了だけでは内容と directory entry の永続化は揃わない](./knowledges/filesystem-sync-durability.md) | Tokio 1.52.3 / Rust std 1.96.0 / Linux man-pages 6.19 / Apple fsync(2) archived manual | 文献のみ | 2026-10-04 |
| [置換しない rename は OS ごとに別の API で提供される](./knowledges/filesystem-output-publication.md) | Linux man-pages 6.19 / macOS 26.6.2 rename(2) / Microsoft Learn MoveFileExW / Rust std 1.96.0 | 文献のみ | 2026-10-04 |

RocksDB と SQLite の同期は、実体より後に metadata を確定する順序の前提である。
filesystem の同期と置換しない rename は、それに加えて購読の出力を確定し、再起動時に配置の有無を判定する手順に関わる。
Windows の directory entry の永続化は rename の項目の `MOVEFILE_WRITE_THROUGH` に依存するため、2 つの filesystem の項目は合わせて読み直す。
各項目の含意から [storage.md](./design/storage.md#1-このドキュメントについて) の依存する節を参照する。
