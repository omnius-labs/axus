use omnius_core_omnikit::generated::omni_sign::OmniCert;

#[repr(u32)]
#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum SessionVersion {
    V1 = 1,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HelloMessage {
    pub version: SessionVersion,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ChallengeMessage {
    pub nonce: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1SignatureMessage {
    pub cert: OmniCert,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum V1RequestType {
    Unknown = 0,
    NodeFinder = 1,
    FileExchanger = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1RequestMessage {
    pub request_type: V1RequestType,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum V1ResultType {
    Unknown,
    Accept,
    Reject,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ResultMessage {
    pub result_type: V1ResultType,
}
