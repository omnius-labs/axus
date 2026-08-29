use omnius_core_omnikit::generated::omni_hash::OmniHash;

// omnius-lint:debt(free-fn) block の保存先 key を表す型が欠けており、型の新設を規約の導入と分ける
pub fn gen_block_path(root_hash: &OmniHash, block_hash: &OmniHash) -> String {
    format!("{root_hash}/{block_hash}")
}
