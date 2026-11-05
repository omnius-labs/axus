use super::api_error::ApiError;

/// A name from a client, used as one path component under a directory the daemon chose.
pub struct FileName;

impl FileName {
    const MAX_BYTES: usize = 255;

    pub fn validate(name: &str) -> std::result::Result<(), ApiError> {
        // `:` names a drive or an alternate data stream on Windows, where joining `C:a` replaces the directory.
        let forbidden = name.contains(['/', '\\', ':', '\0']);
        let mut components = std::path::Path::new(name).components();
        let one_component = matches!(components.next(), Some(std::path::Component::Normal(_))) && components.next().is_none();
        if forbidden || !one_component || name.len() > Self::MAX_BYTES {
            return Err(ApiError::invalid_request("a file name must be one path component of at most 255 bytes"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_path_component_is_accepted() {
        for name in ["a.txt", ".hidden", "..a", "日本語.bin", &"a".repeat(255)] {
            assert!(FileName::validate(name).is_ok(), "{name}");
        }
        for name in ["", ".", "..", "a/b", "../a", "/a", "a\\b", "a\0b", "C:a", "C:\\a", "a:stream", &"a".repeat(256)] {
            assert!(FileName::validate(name).is_err(), "{name}");
        }
    }
}
