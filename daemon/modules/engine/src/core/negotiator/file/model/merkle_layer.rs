use omnius_core_base::ensure_err;
use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

pub struct MerkleLayer {
    pub rank: u32,
    pub hashes: Vec<OmniHash>,
}

impl RocketMessage for MerkleLayer {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        writer.put_u32(value.rank);

        writer.put_u32(value.hashes.len() as u32);
        for v in &value.hashes {
            OmniHash::pack(writer, v, depth + 1)?;
        }

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let get_too_large_err = || RocketPackError::new(RocketPackErrorKind::TooLarge).with_message("len too large");

        let rank = reader.get_u32()?;

        let len = reader.get_u32()?;
        ensure_err!(len > 128, get_too_large_err);

        let mut hashes = Vec::with_capacity(len as usize);
        for _ in 0..len {
            hashes.push(OmniHash::unpack(reader, depth + 1)?);
        }

        Ok(Self { rank, hashes })
    }
}
