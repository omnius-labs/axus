use std::path::{Path, PathBuf};

use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};

use crate::{model::ProfileId, prelude::*};

const SIGNER_FILE_NAME: &str = "signer";
const KEYS_DIR_NAME: &str = "keys";

/// Stores one Profile signing key per Profile ID under `state_dir/profile/keys/<Profile ID>/signer`.
pub(crate) struct ProfileKeyStore {
    keys_dir: PathBuf,
}

impl ProfileKeyStore {
    pub fn new(profile_dir: &Path) -> Self {
        Self {
            keys_dir: profile_dir.join(KEYS_DIR_NAME),
        }
    }

    /// Generates a key for `name` and saves it under the derived Profile ID.
    pub async fn create(&self, name: &str) -> Result<(ProfileId, OmniSigner)> {
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, name)?;
        let id = self.save(&signer).await?;
        Ok((id, signer))
    }

    /// Saves `signer` and returns the Profile ID derived from it.
    pub async fn save(&self, signer: &OmniSigner) -> Result<ProfileId> {
        let id = ProfileId::from_signer(signer)?;
        let dir = self.keys_dir.join(id.as_str());
        tokio::fs::create_dir_all(&dir).await?;

        // Write through a temporary file so a crash does not leave a partial key in place.
        let path = dir.join(SIGNER_FILE_NAME);
        let temp_path = dir.join(format!("{SIGNER_FILE_NAME}.tmp"));
        tokio::fs::write(&temp_path, signer.export()?).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            tokio::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600)).await?;
        }
        tokio::fs::rename(&temp_path, &path).await?;

        Ok(id)
    }

    /// Loads the key of `id` and checks that it still produces that ID.
    pub async fn load(&self, id: &ProfileId) -> Result<OmniSigner> {
        let path = self.keys_dir.join(id.as_str()).join(SIGNER_FILE_NAME);
        let signer = OmniSigner::import(&tokio::fs::read(&path).await?)?;
        let actual = ProfileId::from_signer(&signer)?;
        if &actual != id {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile key does not match the stored profile id"));
        }
        Ok(signer)
    }

    /// Lists every stored key, ordered by Profile ID.
    pub async fn list(&self) -> Result<Vec<(ProfileId, OmniSigner)>> {
        let mut entries = match tokio::fs::read_dir(&self.keys_dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let mut keys = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::new(ErrorKind::InvalidFormat).with_message("profile key directory name is not UTF-8"))?;
            let id = ProfileId::parse(&name).map_err(|e| Error::from_error(e, ErrorKind::InvalidFormat).with_message("profile key directory name is not a Profile ID"))?;
            let signer = self.load(&id).await?;
            keys.push((id, signer));
        }
        keys.sort_by(|(left, _), (right, _)| left.cmp(right));

        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use testresult::TestResult;

    use super::ProfileKeyStore;
    use crate::model::ProfileId;

    #[tokio::test]
    async fn a_created_key_is_loaded_again() -> TestResult {
        let dir = tempfile::tempdir()?;
        let store = ProfileKeyStore::new(dir.path());

        let (id, created) = store.create("Anonymous").await?;
        let loaded = store.load(&id).await?;

        assert_eq!(id.name(), "Anonymous");
        assert_eq!(created.export()?, loaded.export()?);
        assert_eq!(loaded.to_string(), id.to_string());
        Ok(())
    }

    #[tokio::test]
    async fn a_restart_loads_the_same_key_and_id() -> TestResult {
        let dir = tempfile::tempdir()?;
        let created = ProfileKeyStore::new(dir.path()).create("Anonymous").await?;
        let loaded = ProfileKeyStore::new(dir.path()).create("second").await?;
        let first_again = ProfileKeyStore::new(dir.path()).load(&created.0).await?;

        assert_ne!(created.0, loaded.0);
        assert_eq!(created.1.export()?, first_again.export()?);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_key_is_saved_for_the_owner_only() -> TestResult {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir()?;
        let (id, _) = ProfileKeyStore::new(dir.path()).create("Anonymous").await?;
        let mode = tokio::fs::metadata(dir.path().join("keys").join(id.as_str()).join("signer")).await?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        Ok(())
    }

    #[tokio::test]
    async fn a_broken_key_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (id, _) = ProfileKeyStore::new(dir.path()).create("Anonymous").await?;
        tokio::fs::write(dir.path().join("keys").join(id.as_str()).join("signer"), b"broken").await?;

        assert!(ProfileKeyStore::new(dir.path()).load(&id).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn a_key_that_does_not_match_the_stored_id_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (id, _) = ProfileKeyStore::new(dir.path()).create("Anonymous").await?;
        let other = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Other")?;
        tokio::fs::write(dir.path().join("keys").join(id.as_str()).join("signer"), other.export()?).await?;

        assert!(ProfileKeyStore::new(dir.path()).load(&id).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn list_returns_every_key_ordered_by_id() -> TestResult {
        let dir = tempfile::tempdir()?;
        let store = ProfileKeyStore::new(dir.path());
        let (anonymous, _) = store.create("Anonymous").await?;
        let (other, _) = store.create("Other").await?;
        tokio::fs::write(dir.path().join("keys").join("not-a-directory"), b"").await?;

        let ids: Vec<ProfileId> = store.list().await?.into_iter().map(|(id, _)| id).collect();
        let mut expected = vec![anonymous, other];
        expected.sort();
        assert_eq!(ids, expected);
        Ok(())
    }

    #[tokio::test]
    async fn an_invalid_key_directory_name_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        tokio::fs::create_dir_all(dir.path().join("keys").join("not-a-profile-id")).await?;

        assert!(ProfileKeyStore::new(dir.path()).list().await.is_err());
        Ok(())
    }
}
