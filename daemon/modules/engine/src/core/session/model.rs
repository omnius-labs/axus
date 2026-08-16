use omnius_core_omnikit::generated::omni_sign::OmniCert;
use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::base::connection::FramedStream;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionType {
    NodeFinder,
    FileExchanger,
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
