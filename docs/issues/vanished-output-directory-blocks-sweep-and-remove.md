# Failed や Canceled の行の出力先 directory が消えると、sweep の再試行が続き remove もできなくなる

**深刻度: 中**（出力先の directory を消したあとに限る）

調査日 2026-10-07 / 対象コミット `9bbbcbd`

[issues.md](../issues.md) の 1 項目。

## 症状

Failed または Canceled の行の出力先 directory が消えている（外付け disk の取り外しなど）と、次の 2 つが続く。

- sweep が毎回失敗し、再試行が終わらない
- `remove` が失敗し、行を削除できない

## 該当箇所

[store.rs:371](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L371) の `delete_temporary_output` は、一時出力の `NotFound` を「ない」と扱ったあと、出力先 directory の存在を `metadata` で確かめる。directory が消えていると `metadata` が error になり、`delete_temporary_output` は error を返す。

```rust
if !tokio::fs::metadata(&file.output_directory).await?.is_dir() {
    return Err(Error::new(ErrorKind::IoError).with_message("output parent is not a directory"));
}
```

[store.rs:431](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L431) の `sweep_outputs_locked` は Canceled と Failed の行に対して `delete_temporary_output` を呼び、error を返すと `request_sweep` で再試行を予約する。次の sweep でも同じ error になる。
[store.rs:143](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L143) の `remove` は `discard_output`（[同ファイル:356](../../daemon/modules/engine/src/core/negotiator/file/file_subscriber/store.rs#L356)）を呼ぶため、同じ error で行を消せない。

## 原因

一時出力の削除と出力先 directory の存在確認を同じ関数で行い、directory の消失を回復可能な error として返している。
PR #120 で、Completed の行の cancel と remove、および名前が長すぎて一時出力を作れない行は直したが、Failed と Canceled の行は sweep と remove のどちらでも同じ失敗に当たる。

## 影響

出力先 directory を消したままだと、daemon の log に error が繰り返し出て、該当の行は操作から削除できない。
disk を戻せば回復する。
コードを読んで確認した。実行による再現は行っていない。

## 対応方針

Failed と Canceled の行は、出力先 directory が消えている場合の一時出力の不在を「削除済み」として扱う（directory ごと消えているなら、一時出力も消えている）。
出力先 directory の存在確認は、既存の出力の保護が必要な操作（書き出しと確定）に限定する。
出力先 directory が消えた状態で `remove` が成功し、sweep が再試行を続けないことを test する。
