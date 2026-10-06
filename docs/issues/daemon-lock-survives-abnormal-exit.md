# 異常終了した daemon の lock file が残り、再起動できなくなる

**深刻度: 中**（`kill -9`、OS クラッシュ、電源断のあと、手で file を消すまで daemon を起動できない）

調査日 2026-10-06 / 対象コミット `374c397`

[issues.md](../issues.md) の 1 項目。

## 症状

daemon が正常に終了せず `axus.lock` が残っていると、次の起動は lock の取得で `AlreadyExists` の error になり、失敗する。
正常に終了すると、`FileLock` の `Drop` が file を消す。

## 該当箇所

[file_lock.rs:23](../../daemon/refs/core-rs/modules/base/src/file_lock.rs#L23) の `FileLock::acquire` は、`create_new(true)` で file を作る。
file には自分の process ID を書くが、取得するときには読まない。

```rust
OpenOptions::new().write(true).create_new(true).open(path).await?
```

daemon は [executor.rs:113](../../daemon/entrypoints/daemon/src/executor.rs#L113) でこの `FileLock::acquire` を呼ぶ。

## 原因

排他を file の存在だけで判定し、process の生死と結び付けていない。
`Drop` が走らない終了では、file が残る。

## 影響

異常終了した daemon は、利用者が `axus.lock` を手で消すまで再起動しない。
error には lock の path が付くため、消すべき file は分かる。
設定 directory に lock を置いていた変更前も、同じ性質だった。

コードを読んで確認した。実行による再現は行っていない。

## 対応方針

core-rs の `FileLock` を、OS の advisory lock（Linux と macOS は `flock`、Windows は `LockFileEx`）で排他するように改める。
process が終了すると OS が lock を解放するため、file が残っていても次の取得に成功する。
別の process が lock を保持している間は取得が失敗し、その process を `kill` した後は、file が残っていても取得に成功することを test する。
core-rs の変更を先に行い、axus は submodule を更新して使う。
