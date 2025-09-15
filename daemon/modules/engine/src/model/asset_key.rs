use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetKey {
    pub typ: String,
    pub hash: OmniHash,
}

impl RocketMessage for AssetKey {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        writer.put_u32(1);
        writer.put_str(value.typ.to_string().as_str());

        writer.put_u32(2);
        OmniHash::pack(writer, &value.hash, depth + 1)?;

        writer.put_u32(0);

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let mut typ: Option<String> = None;
        let mut hash: Option<OmniHash> = None;

        loop {
            let field_id = reader.get_u32()?;
            if field_id == 0 {
                break;
            }

            match field_id {
                1 => {
                    typ = Some(reader.get_string(1024)?);
                }
                2 => {
                    hash = Some(OmniHash::unpack(reader, depth + 1)?);
                }
                _ => {}
            }
        }

        Ok(Self {
            typ: typ.ok_or_else(|| RocketPackError::builder().kind(RocketPackErrorKind::InvalidFormat).build())?,
            hash: hash.ok_or_else(|| RocketPackError::builder().kind(RocketPackErrorKind::InvalidFormat).build())?,
        })
    }
}
