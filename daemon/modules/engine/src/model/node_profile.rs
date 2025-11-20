use omnius_core_base::ensure_err;
use omnius_core_omnikit::model::OmniAddr;

use crate::{model::converter::UriConverter, prelude::*};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeProfile {
    pub id: Vec<u8>,
    pub addrs: Vec<OmniAddr>,
}

impl std::fmt::Display for NodeProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = UriConverter::encode("node", self).map_err(|_| std::fmt::Error)?;
        write!(f, "{s}")
    }
}

impl std::str::FromStr for NodeProfile {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        UriConverter::decode("node", s)
    }
}

impl RocketPackStruct for NodeProfile {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
    }
}
