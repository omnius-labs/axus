use std::{io, path::Path};

/// 購読の一時出力を、既存 entry を置き換えずに配置する。
pub struct OutputPublication;

impl OutputPublication {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn rename(source: &Path, destination: &Path) -> io::Result<()> {
        use std::{ffi::CString, os::unix::ffi::OsStrExt as _};
        let source = CString::new(source.as_os_str().as_bytes())?;
        let destination = CString::new(destination.as_os_str().as_bytes())?;
        // path はこの呼び出しの間、有効な NUL 終端文字列を指す。
        #[cfg(target_os = "linux")]
        let result = unsafe { libc::renameat2(libc::AT_FDCWD, source.as_ptr(), libc::AT_FDCWD, destination.as_ptr(), libc::RENAME_NOREPLACE) };
        #[cfg(target_os = "macos")]
        let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub fn rename(source: &Path, destination: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
        if source[..source.len() - 1].contains(&0) || destination[..destination.len() - 1].contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"));
        }
        // path は有効な NUL 終端文字列を指す。置換と volume 間の copy は許可しない。
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn sync_directory(directory: &Path) -> io::Result<()> {
        std::fs::File::open(directory)?.sync_all()
    }

    #[cfg(target_os = "windows")]
    pub fn sync_directory(_directory: &Path) -> io::Result<()> {
        // 配置時の MOVEFILE_WRITE_THROUGH が成功した entry は同期済みである。
        Ok(())
    }

    pub fn is_existing_destination(error: &io::Error) -> bool {
        if error.kind() == io::ErrorKind::AlreadyExists {
            return true;
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            matches!(error.raw_os_error(), Some(libc::EEXIST))
        }
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS};
            error
                .raw_os_error()
                .is_some_and(|code| code as u32 == ERROR_ALREADY_EXISTS || code as u32 == ERROR_FILE_EXISTS)
        }
    }

    pub fn is_unsupported(error: &io::Error) -> bool {
        if error.kind() == io::ErrorKind::Unsupported {
            return true;
        }
        #[cfg(target_os = "linux")]
        {
            matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP))
        }
        #[cfg(target_os = "macos")]
        {
            matches!(error.raw_os_error(), Some(libc::ENOTSUP))
        }
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::Foundation::{ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED};
            error
                .raw_os_error()
                .is_some_and(|code| code as u32 == ERROR_INVALID_FUNCTION || code as u32 == ERROR_NOT_SUPPORTED)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testresult::TestResult;

    #[test]
    fn rename_publishes_without_replacement() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::write(&source, b"new")?;
        OutputPublication::rename(&source, &destination)?;
        OutputPublication::sync_directory(dir.path())?;
        assert!(!source.exists());
        assert_eq!(std::fs::read(&destination)?, b"new");
        std::fs::write(&source, b"next")?;
        assert!(OutputPublication::rename(&source, &destination).is_err());
        assert_eq!(std::fs::read(&source)?, b"next");
        assert_eq!(std::fs::read(&destination)?, b"new");
        Ok(())
    }

    #[test]
    fn rename_preserves_existing_directory() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::write(&source, b"new")?;
        std::fs::create_dir(&destination)?;
        std::fs::write(destination.join("child"), b"original")?;
        assert!(OutputPublication::rename(&source, &destination).is_err());
        assert_eq!(std::fs::read(destination.join("child"))?, b"original");
        assert_eq!(std::fs::read(&source)?, b"new");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rename_preserves_existing_symlinks() -> TestResult {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        std::fs::write(&source, b"new")?;
        std::fs::write(&target, b"original")?;
        for name in ["link", "dangling"] {
            let destination = dir.path().join(name);
            let target = if name == "link" { target.clone() } else { dir.path().join("missing") };
            std::os::unix::fs::symlink(&target, &destination)?;
            assert!(OutputPublication::rename(&source, &destination).is_err());
            assert_eq!(std::fs::read_link(&destination)?, target);
        }
        assert_eq!(std::fs::read(&source)?, b"new");
        assert_eq!(std::fs::read(&target)?, b"original");
        Ok(())
    }
}
