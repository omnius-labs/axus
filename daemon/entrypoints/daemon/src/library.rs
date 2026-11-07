use std::path::{Component, Path, PathBuf};

use sha3::{Digest, Sha3_256};

use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

pub struct LibraryEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: i64,
}

pub struct LibraryRoot {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// The configured roots a publication may read, and the containment rules over them.
pub struct LibraryRoots {
    roots: Vec<LibraryRoot>,
}

impl LibraryRoots {
    pub async fn open(paths: &[PathBuf]) -> Result<Self> {
        let mut roots = Vec::new();
        for path in paths {
            let canonical = tokio::fs::canonicalize(path).await?;
            if !tokio::fs::metadata(&canonical).await?.is_dir() {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message(format!("library root {} is not a directory", canonical.display())));
            }
            let name = canonical
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| canonical.to_string_lossy().to_string());
            roots.push(LibraryRoot {
                id: Self::root_id(&canonical),
                name,
                path: canonical,
            });
        }
        Ok(Self { roots })
    }

    pub fn list(&self) -> &[LibraryRoot] {
        &self.roots
    }

    pub fn find(&self, id: &str) -> Option<&LibraryRoot> {
        self.roots.iter().find(|root| root.id == id)
    }

    pub async fn entries(&self, id: &str, relative: &str) -> Result<Vec<LibraryEntry>> {
        let root = self.find(id).ok_or_else(|| Error::new(ErrorKind::NotFound).with_message("no such library root"))?;
        let directory = self.resolve(root, relative)?;
        if !tokio::fs::metadata(&directory).await?.is_dir() {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the library path is not a directory"));
        }
        let mut entries = Vec::new();
        let mut reader = tokio::fs::read_dir(&directory).await?;
        while let Some(entry) = reader.next_entry().await? {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let metadata = entry.metadata().await?;
            let kind = if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File
            } else {
                continue;
            };
            entries.push(LibraryEntry {
                name,
                kind,
                size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    pub fn resolve_file(&self, id: &str, relative: &str) -> Result<(PathBuf, i64, i64)> {
        let root = self.find(id).ok_or_else(|| Error::new(ErrorKind::NotFound).with_message("no such library root"))?;
        let path = self.resolve(root, relative)?;
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the library path is not a regular file"));
        }
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64 * 1_000_000_000 + i64::from(duration.subsec_nanos()))
            .unwrap_or(0);
        Ok((path, i64::try_from(metadata.len()).unwrap_or(i64::MAX), modified_at))
    }

    fn resolve(&self, root: &LibraryRoot, relative: &str) -> Result<PathBuf> {
        let relative = Path::new(relative);
        if relative.is_absolute() {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the library path must be relative"));
        }
        if relative.components().any(|component| !matches!(component, Component::Normal(_) | Component::CurDir)) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the library path must not contain `..`"));
        }
        let joined = root.path.join(relative);
        let canonical = std::fs::canonicalize(&joined)?;
        if !canonical.starts_with(&root.path) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the library path leaves its root"));
        }
        Ok(canonical)
    }

    fn root_id(path: &Path) -> String {
        let mut hasher = Sha3_256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        hex::encode(hasher.finalize())[..16].to_string()
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn a_root_lists_entries_and_rejects_escapes() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.txt"), b"abc")?;
        std::fs::create_dir(dir.path().join("sub"))?;
        let roots = LibraryRoots::open(&[dir.path().to_path_buf()]).await?;
        let root = &roots.list()[0];
        let entries = roots.entries(&root.id, "").await?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].size, 3);
        assert_eq!(entries[1].kind, EntryKind::Directory);
        let (path, size, _) = roots.resolve_file(&root.id, "a.txt")?;
        assert_eq!(path, dir.path().canonicalize()?.join("a.txt"));
        assert_eq!(size, 3);
        assert!(roots.resolve_file(&root.id, "../a.txt").is_err());
        assert!(roots.resolve_file(&root.id, "/etc/passwd").is_err());
        assert!(roots.resolve_file(&root.id, "sub").is_err());
        assert!(roots.entries("unknown", "").await.is_err());
        Ok(())
    }
}
