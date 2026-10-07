use std::time::Duration;

use omnius_core_omnikit::generated::omni_sign::OmniCert;
use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::base::connection::FramedStream;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionType {
    NodeFinder,
    FileExchanger,
}

#[derive(Debug, Clone)]
pub struct SessionOption {
    #[allow(unused)]
    pub handshake_timeout: Duration,
    #[allow(unused)]
    pub max_pending_handshake_count: usize,
    #[allow(unused)]
    pub handshake_max_frame_length: usize,
}

impl SessionOption {
    pub const HANDSHAKE_MAX_FRAME_LENGTH: usize = 16 * 1024;

    pub fn new() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            max_pending_handshake_count: 64,
            handshake_max_frame_length: Self::HANDSHAKE_MAX_FRAME_LENGTH,
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
