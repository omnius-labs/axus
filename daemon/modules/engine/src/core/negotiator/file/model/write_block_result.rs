/// What `FileSubscriber::write_block` did with a received block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteBlockResult {
    /// The content hash matched and the block was saved.
    Stored,
    /// No subscription needs the block, or it is already saved.
    Unneeded,
    /// The content does not match the hash it was carried with. Nothing is saved.
    HashMismatch,
}
