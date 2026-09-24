use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};

use crate::prelude::*;

const SIGNER_NAME: &str = "axus-node";
const SIGNER_FILE_NAME: &str = "signer";

/// node の署名鍵を state directory に保存し、再起動後も同じ鍵と node ID を使う
pub struct NodeIdentity {
    signer: Arc<OmniSigner>,
    public_key: Vec<u8>,
}

impl NodeIdentity {
    /// 保存済みの鍵を読み込み、なければ生成して保存する。
    /// 読み込めない鍵を作り直すと node ID が黙って変わるため、その場合は error を返す。
    pub async fn load_or_create(dir: &Path) -> Result<Self> {
        let path = dir.join(SIGNER_FILE_NAME);
        let signer = match tokio::fs::read(&path).await {
            Ok(bytes) => OmniSigner::import(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, SIGNER_NAME)?;
                Self::save(dir, &path, &signer).await?;
                signer
            }
            Err(e) => return Err(e.into()),
        };

        // cert と同じ表現の公開鍵を得るため、署名の結果から取り出す
        let public_key = signer.sign(&[])?.public_key;

        Ok(Self {
            signer: Arc::new(signer),
            public_key,
        })
    }

    pub fn signer(&self) -> Arc<OmniSigner> {
        self.signer.clone()
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    async fn save(dir: &Path, path: &Path, signer: &OmniSigner) -> Result<()> {
        tokio::fs::create_dir_all(dir).await?;

        // 書き込み途中の file を鍵として読まないよう、一時 file に書いてから置き換える
        let temp_path: PathBuf = dir.join(format!("{SIGNER_FILE_NAME}.tmp"));
        tokio::fs::write(&temp_path, signer.export()?).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            tokio::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600)).await?;
        }
        tokio::fs::rename(&temp_path, path).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::NodeIdentity;

    #[tokio::test]
    async fn load_or_create_restores_the_saved_key() -> TestResult {
        let dir = tempfile::tempdir()?;

        let created = NodeIdentity::load_or_create(dir.path()).await?;
        let loaded = NodeIdentity::load_or_create(dir.path()).await?;
        assert_eq!(created.public_key(), loaded.public_key());

        let other_dir = tempfile::tempdir()?;
        let other = NodeIdentity::load_or_create(other_dir.path()).await?;
        assert_ne!(created.public_key(), other.public_key());

        Ok(())
    }

    #[tokio::test]
    async fn load_or_create_rejects_a_broken_key() -> TestResult {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(dir.path().join("signer"), b"broken").await?;

        assert!(NodeIdentity::load_or_create(dir.path()).await.is_err());

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn load_or_create_saves_the_key_for_the_owner_only() -> TestResult {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir()?;
        NodeIdentity::load_or_create(dir.path()).await?;

        let mode = tokio::fs::metadata(dir.path().join("signer")).await?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        Ok(())
    }
}
