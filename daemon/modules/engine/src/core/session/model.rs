use std::time::Duration;

use omnius_core_omnikit::generated::omni_sign::OmniCert;
use omnius_core_omnikit::model::omni_addr::OmniAddr;
use omnius_core_omnikit::service::connection::secure::OmniSecureStreamOption;

use crate::base::connection::FramedStream;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionType {
    NodeFinder,
    FileExchanger,
}

impl SessionType {
    pub fn max_frame_length(&self) -> usize {
        match self {
            Self::NodeFinder => FramedStream::NODE_FINDER_MAX_FRAME_LENGTH,
            Self::FileExchanger => FramedStream::FILE_EXCHANGER_MAX_FRAME_LENGTH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionOption {
    pub handshake_timeout: Duration,
    pub max_pending_handshake_count: usize,
    pub handshake_max_frame_length: usize,
    pub rekey_after_bytes: u64,
    pub rekey_after_records: u64,
}

impl SessionOption {
    pub const HANDSHAKE_MAX_FRAME_LENGTH: usize = 16 * 1024;
    pub const SECURE_CONTEXT: &'static [u8] = b"axus-session-v2";

    pub(crate) fn secure_option(&self) -> crate::result::Result<OmniSecureStreamOption> {
        let mut option = OmniSecureStreamOption::new(Self::SECURE_CONTEXT)?;
        option.handshake_max_frame_length = self.handshake_max_frame_length;
        option.rekey_after_bytes = self.rekey_after_bytes;
        option.rekey_after_records = self.rekey_after_records;
        Ok(option)
    }

    pub fn new() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            max_pending_handshake_count: 64,
            handshake_max_frame_length: Self::HANDSHAKE_MAX_FRAME_LENGTH,
            rekey_after_bytes: 1 << 30,
            rekey_after_records: 1 << 20,
        }
    }
}

impl Default for SessionOption {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionHandshakeType {
    Connected,
    Accepted,
}

#[derive(Clone)]
pub struct Session {
    #[allow(unused)]
    pub typ: SessionType,
    #[allow(unused)]
    pub address: OmniAddr,
    #[allow(unused)]
    pub handshake_type: SessionHandshakeType,
    #[allow(unused)]
    pub cert: OmniCert,
    pub stream: FramedStream,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::SessionOption;

    #[test]
    fn session_option_defaults_match_input_boundary_limits() {
        let option = SessionOption::default();
        assert_eq!(option.handshake_timeout, Duration::from_secs(10));
        assert_eq!(option.max_pending_handshake_count, 64);
        assert_eq!(option.handshake_max_frame_length, 16 * 1024);
    }
}
