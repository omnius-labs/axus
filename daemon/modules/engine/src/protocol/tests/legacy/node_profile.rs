use crate::prelude::*;
use omnius_core_omnikit::model::omni_addr::OmniAddr;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeProfile {
    pub public_key: Vec<u8>,
    pub addrs: Vec<OmniAddr>,
}
impl NodeProfile {
    pub const MAX_WIRE_ADDRS: usize = 8;
    pub fn new(public_key: Vec<u8>, addrs: Vec<OmniAddr>) -> Self {
        Self { public_key, addrs }
    }
    fn unpack_with_max_addrs(decoder: &mut impl RocketPackDecoder, max_addrs: usize) -> std::result::Result<Self, RocketPackDecoderError> {
        let mut public_key: Option<Vec<u8>> = None;
        let mut addrs: Option<Vec<OmniAddr>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => public_key = Some(decoder.read_bytes_vec()?),
                1 => {
                    let count = decoder.read_array_bounded("NodeProfile.addrs", 0, max_addrs as u64)?;
                    let mut items: Vec<OmniAddr> = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        items.push(OmniAddr::from(decoder.read_string()?));
                    }
                    addrs = Some(items);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self::new(
            public_key.ok_or(RocketPackDecoderError::Other("missing field: public_key"))?,
            addrs.ok_or(RocketPackDecoderError::Other("missing field: addrs"))?,
        ))
    }
}

impl RocketPackStruct for NodeProfile {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(2)?;

        encoder.write_u64(0)?;
        encoder.write_bytes(value.public_key.as_slice())?;

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
        Self::unpack_with_max_addrs(decoder, Self::MAX_WIRE_ADDRS)
    }
}
