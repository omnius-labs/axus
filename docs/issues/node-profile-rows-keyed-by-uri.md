# アドレスが変わった node が既知 node に重複して残る

**深刻度: 低**（node が再起動後に別のアドレスを広告するまで未顕在）

調査日 2026-09-24 / 対象コミット `17cc3f4`

[issues.md](../issues.md) の 1 項目。

## 症状

同じ node の NodeProfile が、アドレスの異なる複数の行として既知 node に残る。
古いアドレスの行を選ぶと、TaskConnector は到達できないアドレスへ接続を試みる。

## 該当箇所

[node_finder_repo.rs:39](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs#L39) は、NodeProfile の URI 全体を主キーにしている。

```sql
CREATE TABLE IF NOT EXISTS node_profiles (
    value TEXT NOT NULL PRIMARY KEY,
```

[node_finder_repo.rs:74](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs#L74) の `INSERT OR IGNORE` は、同じ URI の行だけを重複として扱う。

## 原因

URI は公開鍵とアドレスの両方を含むため、アドレスが変わると同じ node でも別の値になる。
node ID で行を同定していない。

## 影響

古いアドレスの行は、件数上限による削除（[node_finder_repo.rs:93](../../daemon/modules/engine/src/core/negotiator/node/node_finder_repo.rs#L93)）で押し出されるまで残る。
接続に成功すると同じ node ID の行はすべて候補から外れるため、重複した Session は生まれないが、到達できないアドレスへの接続の試行が増える。

## 対応方針

node ID を主キーにした行へ NodeProfile を保存し、同じ node ID の NodeProfile を受け取ったら新しい値で置き換える。
既存の migration は適用済みの状態があり得るため、新しい MigrationRequest で table を移行する。
同じ node ID でアドレスだけが異なる NodeProfile を保存したとき、行が 1 つに保たれることを test する。
