# 購読の復号が既存出力を上書きし、中断時に削除する

**深刻度: 高**（file 転送と REST API は未結線だが、engine の購読を使うと既存データを失い得る）

調査日 2026-10-04 / 対象コミット `84784b2` を基点とする staged 変更後の作業ツリー

[issues.md](../issues.md) の 1 項目。

## 症状

購読の出力先に既存 file があると、rank 0 の復号開始時に内容を切り詰める。
復号の失敗、cancel、Completed への条件付き更新の不一致が起きると、その出力先を削除する。
同じ出力先への購読も重複して登録できるため、先に完成した出力を後の購読が破壊し得る。

## 該当箇所

[task_decoder.rs:126](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L126) は出力先を `File::create` で直接開く。
[同ファイル:139](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/task_decoder.rs#L139) は成功以外の結果で出力先を削除する。
[store.rs:45](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L45) と [repo.rs:39](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/repo.rs#L39) は同じ出力先の予約・一意性を持たない。

## 原因

利用者の出力先と、購読が所有する途中出力を同じ path で扱っている。
既存 entry の保護も、確定時の上書きなし配置もない。

## 影響

既存 file と Completed の出力に対して、内容の消失と file の削除が起き得る。
状態の条件付き更新は購読の行を守るが、すでに行った filesystem の上書きを元に戻せない。
実行によるデータ破壊の再現は行っていない。上記の API と条件分岐から確認した。

## 対応方針

[file-transfer.md §4.2](../design/file-transfer.md#42-購読側) の出力先予約と既存 entry の拒否を実装する。
[storage.md §4.5](../design/storage.md#45-出力の確定と回復) に従い、規約の名前の一時出力、Finalizing、置換しない rename による配置を導入する。
cancel/remove と sweep は購読の行から求めた一時出力だけを消し、確定中の cancel/remove は拒否する。
既存 file、同一出力先の競合、配置直前の外部 entry 作成、中断と再起動の各場合で、既存内容を維持することを確認する。
