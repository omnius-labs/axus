# 購読の出力確定前に file と directory entry の永続化を確保していない

**深刻度: 中**（file 転送と REST API は未結線。engine の完了後に OS クラッシュや電源断が起きた場合の保証不足）

調査日 2026-10-06 / 対象コミット `bbd7111` を基点とする作業ツリー

[issues.md](../issues.md) の 1 項目。

## 症状

購読の復号は出力先へ直接書き込み、writer の `flush` 後に Completed を記録する。
出力 file と親 directory の永続化を確認しないため、OS クラッシュや電源断では Completed の出力内容や配置が失われ得る。

## 該当箇所

[subscriber/task_decoder.rs:126](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L126) は直接出力を開く。
[同ファイル:184](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L184) は writer を `flush` し、[同ファイル:131](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L131) で Completed を記録する。

## 原因

出力の I/O 完了を、電源断後にも保持できる永続化の完了と同じものとして扱っている。
[file と directory の同期](../knowledges/filesystem-sync-durability.md) は別々の条件を持ち、writer の `flush` だけでは両方を満たさない。
一時出力を永続化してから配置を確定する手順と、配置の途中で終了した場合の回復もない。

## 影響

Completed の出力が障害後に欠落し得る。
現在の起動時回復は block の orphan key の回収であり、出力の配置を回復しない。
この指摘はコードと公式仕様の照合による。
OS クラッシュと電源断の実機試験は未実施である。

## 対応方針

[storage.md §4.2](../design/storage.md#42-永続化の保証と境界) と [§4.5](../design/storage.md#45-出力の確定と回復) に従い、一時出力を `flush` と `sync_all` で永続化した後、Finalizing を記録する。
既存 entry を置き換えない rename で配置し、Linux と macOS では親 directory を同期し、Windows では配置に `MOVEFILE_WRITE_THROUGH` を指定する。
Completed は配置の永続化後に記録する。
同期失敗時は成功を返さず、Finalizing の配置と回復を再試行する。
状態遷移の境界ごとの再起動と、対応環境ごとの同期条件を検証する。
