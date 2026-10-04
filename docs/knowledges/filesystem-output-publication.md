# 置換しない rename は OS ごとに別の API で提供される

**出所: 文献のみ**（Linux man-pages 6.19 / macOS 26.6.2 rename(2) / Microsoft Learn MoveFileExW・MoveFile / Rust std 1.96.0 ソース / 2026-10-04）

[knowledges.md](../knowledges.md) の 1 項目。

## 事実

Rust の `std::fs::rename` は、Unix では `rename`、Windows では `MoveFileExW` または `SetFileInformationByHandle` に対応し、既存の file の追加先を置き換える。
既存 entry があれば失敗する rename は、OS ごとに次の API で得られる。

| OS | API | 追加先が既存のとき | 保存先が対応しないとき |
| --- | --- | --- | --- |
| Linux | `renameat2` の `RENAME_NOREPLACE` | `EEXIST` で失敗する | `EINVAL` で失敗する |
| macOS | `renamex_np` の `RENAME_EXCL` | `EEXIST` で失敗する | `ENOTSUP` で失敗する |
| Windows | `MOVEFILE_REPLACE_EXISTING` を指定しない `MoveFileExW` | 失敗する | 資料に記載がない |

Linux の `RENAME_NOREPLACE` は、man-pages の記載では ext4（Linux 3.15）、btrfs・tmpfs・cifs（3.17）、xfs（4.0）、ext2・vfat ほか（4.9）が対応する。
macOS の `RENAME_EXCL` は、`getattrlist(2)` の `VOL_CAP_INT_RENAME_EXCL` を持つ filesystem で有効になる。
Windows の `MOVEFILE_WRITE_THROUGH` を指定した `MoveFileExW` は、移動が disk に反映されるまで戻らない。
`MOVEFILE_REPLACE_EXISTING` を指定しない場合の既存の追加先の扱いは、`MoveFileExW` の資料ではフラグの説明の反対として読み取れ、`MoveFile` の資料は追加先が存在してはならないと記す。

どの API も、rename は同じ filesystem または volume の中で 1 つの名前を別の名前に付け替える。
Windows の `MOVEFILE_COPY_ALLOWED` を指定すると volume をまたぐ移動が copy と削除になるが、付け替えではなくなる。

## 根拠

[Linux man-pages 6.19 rename(2)](https://man7.org/linux/man-pages/man2/rename.2.html)、macOS 26.6.2 の `man 2 rename`、[Microsoft Learn MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)、[Microsoft Learn MoveFile](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefile) を 2026-10-04 に参照した。
`std::fs::rename` の対応先は Rust 1.96.0 の配布ソース `library/std/src/fs.rs` の文書 comment で確認した。
各 OS での実行と、電源断の試験は行っていない。

## 含意

[storage.md §4.5](../design/storage.md#45-出力の確定と回復) は、一時出力を置換しない rename で出力先へ配置し、一時出力と出力先の有無から配置の有無を判定する。
保存先が対応しない場合の失敗は、§4.5 で Failed として扱う。
Windows の directory entry の永続化は、[storage.md §4.2](../design/storage.md#42-永続化の保証と境界) で `MOVEFILE_WRITE_THROUGH` に依存する。
