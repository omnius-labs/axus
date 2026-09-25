# 別の設定 directory から同じ state directory を使う daemon を止められない

**深刻度: 低**（同じ `state_dir` を指す設定 directory を 2 つ作ったときだけ起きる）

調査日 2026-09-25 / 対象コミット `59054a2`

[issues.md](../issues.md) の 1 項目。

## 症状

`core.state_dir` が同じで設定 directory だけが異なる 2 つの daemon を同時に起動できる。
2 つの daemon は同じ署名鍵と既知 node の SQLite を使う。

## 該当箇所

[executor.rs:85](../../daemon/entrypoints/daemon/src/executor.rs#L85) は設定 directory の中の `axus.lock` を lock する。

```rust
let _lock = FileLock::acquire(config_dir.join("axus.lock")).await?;
```

## 原因

多重起動を防ぐ lock が、守るべき永続状態ではなく設定 directory に置かれている。

## 影響

同じ node ID を持つ 2 つの node が別の到達先を広告し、互いの既知 node の書き込みが混ざる。
RocksDB は自身の lock で 2 つ目の open を拒否するため、FileExchanger を結線した後は 2 つ目の daemon の起動が失敗する。

## 対応方針

lock を `state_dir` の中に置く。
同じ `state_dir` を指す 2 つの設定で、2 つ目の起動が lock の取得で失敗することを test する。
