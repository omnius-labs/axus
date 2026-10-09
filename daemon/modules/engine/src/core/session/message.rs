#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum SessionVersion {
    V2 = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HelloMessage {
    pub version: SessionVersion,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum V2RequestType {
    Unknown = 0,
    NodeFinder = 1,
    FileExchanger = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V2RequestMessage {
    pub request_type: V2RequestType,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum V2ResultType {
    Unknown = 0,
    Accept = 1,
    Reject = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V2ResultMessage {
    pub result_type: V2ResultType,
}
