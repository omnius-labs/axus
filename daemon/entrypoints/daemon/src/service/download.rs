use std::path::PathBuf;

pub struct DownloadInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub etag: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteRange {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

// omnius-lint:allow(free-fn) stateless HTTP byte-range and header helper
pub fn parse_range(header: Option<&str>, size: u64) -> ByteRange {
    let Some(header) = header else {
        return ByteRange::Full;
    };
    let Some(value) = header.strip_prefix("bytes=") else {
        return ByteRange::Unsatisfiable;
    };
    if value.contains(',') {
        return ByteRange::Unsatisfiable;
    }
    let Some((start, end)) = value.split_once('-') else {
        return ByteRange::Unsatisfiable;
    };
    if size == 0 {
        return ByteRange::Unsatisfiable;
    }
    let (start, end) = if start.is_empty() {
        let Ok(suffix) = end.parse::<u64>() else {
            return ByteRange::Unsatisfiable;
        };
        if suffix == 0 {
            return ByteRange::Unsatisfiable;
        }
        (size.saturating_sub(suffix), size - 1)
    } else {
        let Ok(start) = start.parse::<u64>() else {
            return ByteRange::Unsatisfiable;
        };
        let end = if end.is_empty() {
            size - 1
        } else {
            match end.parse::<u64>() {
                Ok(end) => end.min(size - 1),
                Err(_) => return ByteRange::Unsatisfiable,
            }
        };
        (start, end)
    };
    if start > end || start >= size {
        return ByteRange::Unsatisfiable;
    }
    ByteRange::Partial { start, end }
}

// omnius-lint:allow(free-fn) stateless HTTP byte-range and header helper
pub fn content_disposition(file_name: &str) -> String {
    let ascii: String = file_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{}", percent_encode(file_name))
}

// omnius-lint:allow(free-fn) stateless HTTP byte-range and header helper
fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_parsing_test() {
        assert_eq!(parse_range(None, 10), ByteRange::Full);
        assert_eq!(parse_range(Some("bytes=0-4"), 10), ByteRange::Partial { start: 0, end: 4 });
        assert_eq!(parse_range(Some("bytes=5-"), 10), ByteRange::Partial { start: 5, end: 9 });
        assert_eq!(parse_range(Some("bytes=-3"), 10), ByteRange::Partial { start: 7, end: 9 });
        assert_eq!(parse_range(Some("bytes=5-100"), 10), ByteRange::Partial { start: 5, end: 9 });
        assert_eq!(parse_range(Some("bytes=10-"), 10), ByteRange::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=0-1,3-4"), 10), ByteRange::Unsatisfiable);
        assert_eq!(parse_range(Some("items=0-1"), 10), ByteRange::Unsatisfiable);
    }
}
