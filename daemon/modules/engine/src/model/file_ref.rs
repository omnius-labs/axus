use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileRef {
    pub name: String,
    pub hash: OmniHash,
}

impl RocketMessage for FileRef {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        writer.put_u32(1);
        writer.put_str(&value.name);

        writer.put_u32(2);
        OmniHash::pack(writer, &value.hash, depth + 1)?;

        writer.put_u32(0);

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let mut name: Option<String> = None;
        let mut hash: Option<OmniHash> = None;

        loop {
            let field_id = reader.get_u32()?;
            if field_id == 0 {
                break;
            }

            match field_id {
                1 => {
                    name = Some(reader.get_string(1024)?);
                }
                2 => {
                    hash = Some(OmniHash::unpack(reader, depth + 1)?);
                }
                _ => {}
            }
        }

        Ok(Self {
            name: name.ok_or_else(|| RocketPackError::builder().kind(RocketPackErrorKind::InvalidFormat).build())?,
            hash: hash.ok_or_else(|| RocketPackError::builder().kind(RocketPackErrorKind::InvalidFormat).build())?,
        })
    }
}
