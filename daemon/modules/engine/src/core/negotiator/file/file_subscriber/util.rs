use omnius_core_omnikit::generated::omni_hash::OmniHash;

pub fn gen_block_path(root_hash: &OmniHash, block_hash: &OmniHash) -> String {
    format!("{root_hash}/{block_hash}")
}
