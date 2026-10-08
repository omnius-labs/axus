use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::{generated::axus::model as wire, model::NodeProfile, prelude::*};

use super::{MessageCodec, uri::UriConverter};

pub(crate) struct NodeProfileCodec;

impl MessageCodec for NodeProfileCodec {
    type Message = NodeProfile;
    type Wire = wire::NodeProfile;

    fn to_wire(value: &NodeProfile) -> Self::Wire {
        Self::Wire {
            public_key: value.public_key().to_vec(),
            addrs: value.addrs.iter().map(|addr| addr.as_str().to_owned()).collect(),
        }
    }

    fn from_wire(value: Self::Wire) -> std::result::Result<NodeProfile, RocketPackDecoderError> {
        Ok(NodeProfile::new(value.public_key, value.addrs.into_iter().map(OmniAddr::from).collect()))
    }
}

impl NodeProfile {
    pub const MAX_WIRE_ADDRS: usize = wire::MAX_WIRE_ADDRS as usize;

    pub fn to_uri(&self) -> Result<String> {
        UriConverter::encode("node", &NodeProfileCodec::to_wire(self))
    }
}

impl std::str::FromStr for NodeProfile {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Ok(NodeProfileCodec::from_wire(UriConverter::decode::<wire::NodeProfile>("node", text)?)?)
    }
}
