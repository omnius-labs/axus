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
        encoder.write_map(2)?;

        encoder.write_u64(0)?;
        encoder.write_bytes(value.id.as_slice())?;

        encoder.write_u64(1)?;
        encoder.write_array(value.addrs.len())?;
        for addr in value.addrs.iter() {
            encoder.write_string(addr.as_str())?;
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut id: Option<Vec<u8>> = None;
        let mut addrs: Option<Vec<OmniAddr>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => id = Some(decoder.read_bytes_vec()?),
                1 => {
                    let count = decoder.read_array()?;
                    let mut items: Vec<OmniAddr> = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        items.push(OmniAddr::from(decoder.read_string()?));
                    }
                    addrs = Some(items);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            id: id.ok_or(RocketPackDecoderError::Other("missing field: id"))?,
            addrs: addrs.ok_or(RocketPackDecoderError::Other("missing field: addrs"))?,
        })
    }
}
