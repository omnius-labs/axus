use omnius_core_omnikit::model::OmniHash;

pub fn gen_uncommitted_block_path(id: &str, block_hash: &OmniHash) -> String {
    format!("U/{id}/{block_hash}")
}

pub fn gen_committed_block_path(root_hash: &OmniHash, block_hash: &OmniHash) -> String {
    format!("C/{root_hash}/{block_hash}")
}
