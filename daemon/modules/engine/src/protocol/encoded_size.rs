use omnius_core_rocketpack::RocketPackBytesEncoder;
use std::io::{self, Write};

use crate::prelude::*;

/// 生成 pack と同じ表現の byte 数を、payload を確保せずに数える。
#[derive(Default)]
pub(crate) struct EncodedSize {
    pub len: usize,
}

impl EncodedSize {
    pub fn of<T: RocketPackStruct>(value: &T) -> std::result::Result<usize, RocketPackEncoderError> {
        let mut counter = Self::default();
        T::pack(&mut RocketPackBytesEncoder::new(&mut counter), value)?;
        Ok(counter.len)
    }
}

impl Write for EncodedSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.len = self.len.checked_add(bytes.len()).ok_or_else(|| io::Error::other("encoded size overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
