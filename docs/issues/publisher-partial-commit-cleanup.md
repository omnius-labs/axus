# 公開 commit の途中失敗で移動済み block が稼働中に回収されない

**深刻度: 低**（file 転送と REST API は未結線。engine の commit 失敗後に未参照 block が容量を占有する）

調査日 2026-10-04 / 対象コミット `84784b2` を基点とする staged 変更後の作業ツリー

[issues.md](../issues.md) の 1 項目。

## 症状

複数 block の一部を committed key へ移した後、後続の rename または SQLite の commit が失敗すると、移動済み key が残る。
終了時の cleanup は uncommitted key だけを消すため、移動済み block は再起動時の回復か同じ root の後続 commit まで残る。
cleanup 自体の失敗も log に残すだけで、稼働中に再試行する永続的な対象を持たない。

## 該当箇所

[store.rs:183](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L183) は block ごとに rename し、[同ファイル:197](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L197) で最後に metadata を確定する。
[同ファイル:143](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/store.rs#L143) の cleanup は `U/{id}/` に限る。
[task_encoder.rs:95](../../daemon/modules/engine/src/core/negotiator/file/file_publisher/task_encoder.rs#L95) は失敗を記録した後に cleanup を 1 回試み、失敗は log に記録する。

## 原因

移動前に対象 root と回収範囲を記録しておらず、cleanup が一時 ID から uncommitted 領域しか特定できない。
回収失敗を次の Task 実行へ引き継ぐ情報もない。

## 影響

稼働を続けたまま失敗が重なると、不要な block が容量を占有する。
移動済み key が残る分岐と回収範囲を静的に確認した。I/O 障害の注入試験は未実施である。

## 対応方針

[storage.md §4.3](../design/storage.md#43-公開の-commit-と回収) に従い、1 回の commit の rename を 1 つの RocksDB transaction にまとめる。
参照のない key は Store の mutex 内で SQLite と照合する sweep で回収し、失敗した間は TaskEncoder が sweep を再実行する。
Failed の再 import は新しい一時 ID で行う。
rename の失敗、metadata commit の失敗、sweep の再失敗、同一 root の共有で、回収対象と保持される block を確認する。
