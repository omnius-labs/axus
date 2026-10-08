use crate::model::NodeProfile;
use enumflags2::BitFlags;

#[repr(u32)]
#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub(crate) enum NodeFinderVersion {
    V1 = 1,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HelloMessage {
    pub version: BitFlags<NodeFinderVersion>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProfileMessage {
    pub node_profile: NodeProfile,
}
