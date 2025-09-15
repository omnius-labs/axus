use std::{path::Path, sync::Arc};

use tokio_util::bytes::Bytes;

use omnius_core_base::tsid::TsidProvider;

use crate::prelude::*;

pub struct KeyValueRocksdbStorage {
    db: Arc<rocksdb::TransactionDB<rocksdb::MultiThreaded>>,
    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
}

impl KeyValueRocksdbStorage {
    #[allow(unused)]
    pub async fn new<P: AsRef<Path>>(state_dir: P, tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>) -> Result<Self> {
        tokio::fs::create_dir_all(&state_dir).await?;

        let mut db_opts = rocksdb::Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.set_atomic_flush(true);

        let names_opts = rocksdb::Options::default();

        let metas_opts = rocksdb::Options::default();

        let mut blocks_opts = rocksdb::Options::default();
        blocks_opts.set_enable_blob_files(true);
        blocks_opts.set_enable_blob_gc(true);
        blocks_opts.set_blob_compression_type(rocksdb::DBCompressionType::None);

        let cfs = vec![
            rocksdb::ColumnFamilyDescriptor::new(rocksdb::DEFAULT_COLUMN_FAMILY_NAME, rocksdb::Options::default()),
            rocksdb::ColumnFamilyDescriptor::new("names", names_opts),
            rocksdb::ColumnFamilyDescriptor::new("metas", metas_opts),
            rocksdb::ColumnFamilyDescriptor::new("blocks", blocks_opts),
        ];

        let txn_db_opts = rocksdb::TransactionDBOptions::default();

        let db = Arc::new(rocksdb::TransactionDB::open_cf_descriptors(&db_opts, &txn_db_opts, state_dir, cfs)?);

        Ok(Self { db, tsid_provider })
    }

    #[allow(unused)]
    pub async fn rename_key<K>(&self, old_key: K, new_key: K, overwrite: bool) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let old_name = old_key.as_ref().to_vec();
        let new_name = new_key.as_ref().to_vec();

        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let txn = db.transaction();

            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let (old_id, new_id) = if old_name <= new_name {
                let old_id = txn.get_for_update_cf(&cf_names, &old_name, true)?;
                let new_id = txn.get_for_update_cf(&cf_names, &new_name, true)?;
                (old_id, new_id)
            } else {
                let new_id = txn.get_for_update_cf(&cf_names, &new_name, true)?;
                let old_id = txn.get_for_update_cf(&cf_names, &old_name, true)?;
                (old_id, new_id)
            };

            if let Some(old_id) = old_id {
                if let Some(new_id) = new_id {
                    if overwrite {
                        txn.delete_cf(&cf_blocks, &new_id)?;
                        txn.delete_cf(&cf_metas, &new_id)?;

                        txn.put_cf(&cf_names, &new_name, &old_id)?;
                        txn.delete_cf(&cf_names, &old_name)?;
                    } else {
                        return Err(Error::builder().kind(ErrorKind::AlreadyExists).build());
                    }
                } else {
                    txn.put_cf(&cf_names, new_name, &old_id)?;
                    txn.delete_cf(&cf_names, old_name)?;
                }
            } else {
                return Err(Error::builder().kind(ErrorKind::NotFound).build());
            }

            txn.commit()?;

            Ok(())
        })
        .await?
    }

    #[allow(unused)]
    pub async fn contains_key<K>(&self, key: K) -> Result<bool>
    where
        K: AsRef<[u8]>,
    {
        let name = key.as_ref().to_vec();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<bool> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let res = db.get_cf(&cf_names, &name)?;
            Ok(res.is_some())
        })
        .await?
    }

    #[allow(unused)]
    pub fn get_keys(&self) -> Result<BlobStorageKeyIterator<'_>> {
        let cf_names = self.db.cf_handle("names").expect("missing CF");
        let mut iter = self.db.raw_iterator_cf(&cf_names);
        iter.seek_to_first();
        let iter = BlobStorageKeyIterator::new(iter);
        Ok(iter)
    }

    #[allow(unused)]
    pub async fn put_value_with_meta<K>(&self, key: K, value: Bytes, meta: Bytes, overwrite: bool) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let name = key.as_ref().to_vec();
        let block = value.clone();
        let meta = meta.clone();
        let db = self.db.clone();
        let tsid_provider = self.tsid_provider.clone();

        tokio::task::spawn_blocking(move || {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let txn = db.transaction();

            if !overwrite {
                let id = match txn.get_cf(&cf_names, &name)? {
                    Some(id) => id,
                    None => return Err(Error::builder().kind(ErrorKind::AlreadyExists).build()),
                };

                txn.put_cf(&cf_blocks, &id, &block)?;
                txn.put_cf(&cf_metas, &id, &meta)?;
            } else {
                let id = match txn.get_cf(&cf_names, &name)? {
                    Some(id) => id,
                    None => {
                        let mut tsid_provider = tsid_provider.lock();
                        let tsid = tsid_provider.create();
                        let id = tsid.to_string().into_bytes();
                        txn.put_cf(&cf_names, &name, &id)?;
                        id
                    }
                };

                txn.put_cf(&cf_blocks, &id, &block)?;
                txn.put_cf(&cf_metas, &id, &meta)?;
            }

            txn.commit()?;

            Ok(())
        })
        .await?
    }

    #[allow(unused)]
    pub async fn get_value_with_meta<K>(&self, name: K) -> Result<Option<StoredBlob>>
    where
        K: AsRef<[u8]>,
    {
        let name = name.as_ref().to_vec();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<Option<StoredBlob>> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let id = match db.get_cf(&cf_names, &name)? {
                Some(id) => id,
                None => return Ok(None),
            };

            let Some(value) = db.get_cf(&cf_blocks, &id)? else {
                return Ok(None);
            };

            let meta = db.get_cf(&cf_metas, &id)?;

            Ok(Some(StoredBlob { value, meta }))
        })
        .await?
    }

    #[allow(unused)]
    pub async fn put_value<K>(&self, key: K, value: Bytes, overwrite: bool) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let name = key.as_ref().to_vec();
        let block = value.clone();
        let db = self.db.clone();
        let tsid_provider = self.tsid_provider.clone();

        tokio::task::spawn_blocking(move || {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");

            let txn = db.transaction();

            if !overwrite {
                let id = match txn.get_cf(&cf_names, &name)? {
                    Some(id) => id,
                    None => return Err(Error::builder().kind(ErrorKind::AlreadyExists).build()),
                };

                txn.put_cf(&cf_blocks, &id, &block)?;
            } else {
                let id = match txn.get_cf(&cf_names, &name)? {
                    Some(id) => id,
                    None => {
                        let mut tsid_provider = tsid_provider.lock();
                        let tsid = tsid_provider.create();
                        let id = tsid.to_string().into_bytes();
                        txn.put_cf(&cf_names, &name, &id)?;
                        id
                    }
                };

                txn.put_cf(&cf_blocks, &id, &block)?;
            }

            txn.commit()?;

            Ok(())
        })
        .await?
    }

    #[allow(unused)]
    pub async fn get_value<K>(&self, name: K) -> Result<Option<Vec<u8>>>
    where
        K: AsRef<[u8]>,
    {
        let name = name.as_ref().to_vec();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");

            let id = match db.get_cf(&cf_names, &name)? {
                Some(id) => id,
                None => return Ok(None),
            };

            let value = db.get_cf(&cf_blocks, &id)?;

            Ok(value)
        })
        .await?
    }

    #[allow(unused)]
    pub async fn put_meta<K>(&self, name: K, meta: Bytes) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let name = name.as_ref().to_vec();
        let meta = meta.clone();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let txn = db.transaction();

            let id = match txn.get_cf(&cf_names, &name)? {
                Some(id) => id,
                None => return Err(Error::builder().kind(ErrorKind::NotFound).message("key is not found").build()),
            };

            txn.put_cf(&cf_metas, &id, &meta)?;
            txn.commit()?;

            Ok(())
        })
        .await?
    }

    #[allow(unused)]
    pub async fn get_meta<K>(&self, name: K) -> Result<Option<Vec<u8>>>
    where
        K: AsRef<[u8]>,
    {
        let name = name.as_ref().to_vec();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let id = match db.get_cf(&cf_names, &name)? {
                Some(id) => id,
                None => return Ok(None),
            };

            let value = db.get_cf(&cf_metas, &id)?;

            Ok(value)
        })
        .await?
    }

    #[allow(unused)]
    pub async fn delete<K>(&self, name: K) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let name = name.as_ref().to_vec();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let txn = db.transaction();

            let id = match txn.get_cf(&cf_names, &name)? {
                Some(id) => id,
                None => return Ok(()),
            };

            txn.delete_cf(&cf_names, name)?;
            txn.delete_cf(&cf_metas, &id)?;
            txn.delete_cf(&cf_blocks, &id)?;

            txn.commit()?;

            Ok(())
        })
        .await?
    }

    #[allow(unused)]
    pub async fn delete_bulk<K>(&self, names: &[K]) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let names_owned: Vec<Vec<u8>> = names.iter().map(|n| n.as_ref().to_vec()).collect();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let mut batch = rocksdb::WriteBatchWithTransaction::default();

            for name in &names_owned {
                let id = match db.get_cf(&cf_names, name)? {
                    Some(id) => id,
                    None => return Ok(()),
                };

                batch.delete_cf(&cf_names, name);
                batch.delete_cf(&cf_metas, &id);
                batch.delete_cf(&cf_blocks, &id);
            }

            db.write(batch)?;

            Ok(())
        })
        .await?
    }

    #[allow(unused)]
    pub async fn shrink<T>(&self, exclude_key_fn: T) -> Result<()>
    where
        T: Fn(&[u8]) -> bool + Send + Sync + 'static,
    {
        let db = self.db.clone();
        #[allow(clippy::type_complexity)]
        let func: Arc<dyn Fn(&[u8]) -> bool + Send + Sync> = Arc::new(exclude_key_fn);

        tokio::task::spawn_blocking(move || -> Result<()> {
            let cf_names = db.cf_handle("names").expect("missing CF");
            let cf_blocks = db.cf_handle("blocks").expect("missing CF");
            let cf_metas = db.cf_handle("metas").expect("missing CF");

            let mut iter = db.raw_iterator_cf(&cf_names);
            iter.seek_to_first();

            let mut batch = rocksdb::WriteBatchWithTransaction::default();

            while let Some(name) = iter.key()
                && let Some(id) = iter.value()
            {
                if !(func)(name) {
                    batch.delete_cf(&cf_names, name);
                    batch.delete_cf(&cf_metas, id);
                    batch.delete_cf(&cf_blocks, id);
                }
                iter.next();
            }

            db.write(batch)?;

            Ok(())
        })
        .await?
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StoredBlob {
    pub value: Vec<u8>,
    pub meta: Option<Vec<u8>>,
}

pub struct BlobStorageKeyIterator<'a> {
    iter: rocksdb::DBRawIteratorWithThreadMode<'a, rocksdb::TransactionDB<rocksdb::MultiThreaded>>,
}

impl<'a> BlobStorageKeyIterator<'a> {
    fn new(iter: rocksdb::DBRawIteratorWithThreadMode<'a, rocksdb::TransactionDB<rocksdb::MultiThreaded>>) -> Self {
        Self { iter }
    }
}

impl Iterator for BlobStorageKeyIterator<'_> {
    type Item = Box<[u8]>;

    fn next(&mut self) -> Option<Box<[u8]>> {
        let key = self.iter.key();
        if let Some(key) = key {
            let key: Box<[u8]> = Box::from(key);
            self.iter.next();
            Some(key)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use tempfile::tempdir;
    use testresult::TestResult;
    use tokio_util::bytes::Bytes;

    use omnius_core_base::{clock::FakeClockUtc, random_bytes::FakeRandomBytesProvider, tsid::TsidProviderImpl};

    use super::*;

    // Helper function to create a test storage instance
    async fn create_test_storage() -> TestResult<(tempfile::TempDir, KeyValueRocksdbStorage)> {
        let temp_dir = tempdir()?;
        let clock = FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into());
        let tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>> = Arc::new(Mutex::new(TsidProviderImpl::new(clock, FakeRandomBytesProvider::new(), 8)));
        let storage = KeyValueRocksdbStorage::new(temp_dir.path(), tsid_provider.clone()).await?;
        Ok((temp_dir, storage))
    }

    #[tokio::test]
    async fn test_rename_key() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // rename: source key doesn't exist
        let res = storage.rename_key("non_existent", "new_name", false).await;
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), &ErrorKind::NotFound);

        // rename: name1 -> name3 (non-existing)
        storage.put_value("name1", Bytes::from_static(b"value1"), true).await?;
        storage.rename_key("name1", "name3", false).await?;
        assert_eq!(storage.get_value("name1").await?, None);
        assert_eq!(storage.get_value("name3").await?, Some(b"value1".to_vec()));

        // rename: when dest exists and overwrite = false -> AlreadyExists
        storage.put_value("name4", Bytes::from_static(b"v4"), true).await?;
        storage.put_value("name5", Bytes::from_static(b"v5"), true).await?;
        let res = storage.rename_key("name5", "name4", false).await;
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), &ErrorKind::AlreadyExists);

        // rename: when dest exists and overwrite = true -> dest replaced
        storage.rename_key("name5", "name4", true).await?;
        assert_eq!(storage.get_value("name5").await?, None);
        assert_eq!(storage.get_value("name4").await?, Some(b"v5".to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_contains_key() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // Test key that doesn't exist
        let exists = storage.contains_key("key_not_exist").await?;
        assert!(!exists);

        // Test key that exists
        storage.put_value("test_key", Bytes::from_static(b"test_value"), true).await?;
        let exists = storage.contains_key("test_key").await?;
        assert!(exists);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_keys() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // Initially no keys
        let names: Vec<Box<[u8]>> = storage.get_keys()?.collect();
        assert_eq!(names.len(), 0);

        // Add some keys
        storage.put_value("key1", Bytes::from_static(b"value1"), true).await?;
        storage.put_value("key2", Bytes::from_static(b"value2"), true).await?;

        // Check that keys are returned
        let names: Vec<Box<[u8]>> = storage.get_keys()?.collect();
        let names_vec: Vec<Vec<u8>> = names.into_iter().map(|b| b.into_vec()).collect();
        assert_eq!(names_vec.len(), 2);
        assert!(names_vec.iter().any(|n| n == b"key1"));
        assert!(names_vec.iter().any(|n| n == b"key2"));

        Ok(())
    }

    #[tokio::test]
    async fn test_put_value_with_meta() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // put_value_with_meta (create) with overwrite = true
        storage
            .put_value_with_meta("name1", Bytes::from_static(b"value1"), Bytes::from_static(b"meta1"), true)
            .await?;
        let v = storage.get_value("name1").await?;
        assert_eq!(v, Some(b"value1".to_vec()));
        let m = storage.get_meta("name1").await?;
        assert_eq!(m, Some(b"meta1".to_vec()));

        // put_value_with_meta with overwrite = false on existing key should update the block and meta (same id)
        storage
            .put_value_with_meta("name1", Bytes::from_static(b"value2"), Bytes::from_static(b"meta2"), false)
            .await?;
        let v = storage.get_value("name1").await?;
        assert_eq!(v, Some(b"value2".to_vec()));
        let m = storage.get_meta("name1").await?;
        assert_eq!(m, Some(b"meta2".to_vec()));

        // put_value_with_meta with overwrite = true on existing key should update the block and meta (same id)
        storage
            .put_value_with_meta("name1", Bytes::from_static(b"value3"), Bytes::from_static(b"meta3"), true)
            .await?;
        let v = storage.get_value("name1").await?;
        assert_eq!(v, Some(b"value3".to_vec()));
        let m = storage.get_meta("name1").await?;
        assert_eq!(m, Some(b"meta3".to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_value_with_meta() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // get_value_with_meta for non-existent key
        let result = storage.get_value_with_meta("non_existent").await?;
        assert_eq!(result, None);

        // get_value_with_meta for existing key without meta
        storage.put_value("name_without_meta", Bytes::from_static(b"value"), true).await?;
        let result = storage.get_value_with_meta("name_without_meta").await?;
        assert!(result.is_some());
        let pair = result.unwrap();
        assert_eq!(pair.value, b"value");
        assert_eq!(pair.meta, None);

        // get_value_with_meta for existing key with meta
        storage
            .put_value_with_meta("name_with_meta", Bytes::from_static(b"value_with_meta"), Bytes::from_static(b"meta1"), true)
            .await?;
        let result = storage.get_value_with_meta("name_with_meta").await?;
        assert!(result.is_some());
        let pair = result.unwrap();
        assert_eq!(pair.value, b"value_with_meta");
        assert_eq!(pair.meta, Some(b"meta1".to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_value() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // put_value (create) with overwrite = true
        storage.put_value("name1", Bytes::from_static(b"value1"), true).await?;
        let v = storage.get_value("name1").await?;
        assert_eq!(v, Some(b"value1".to_vec()));

        // put_value with meta
        storage
            .put_value_with_meta("name_with_meta", Bytes::from_static(b"value_with_meta"), Bytes::from_static(b"meta1"), true)
            .await?;
        let v = storage.get_value("name_with_meta").await?;
        assert_eq!(v, Some(b"value_with_meta".to_vec()));
        let m = storage.get_meta("name_with_meta").await?;
        assert_eq!(m, Some(b"meta1".to_vec()));

        // put_value with overwrite = false on existing key should update the block (same id)
        storage.put_value("name1", Bytes::from_static(b"value2"), false).await?;
        let v = storage.get_value("name1").await?;
        assert_eq!(v, Some(b"value2".to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_put_meta() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // put_meta for non-existent key should return NotFound
        let meta = Bytes::from_static(b"meta");
        let res = storage.put_meta("non_existent", meta.clone()).await;
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), &ErrorKind::NotFound);

        // put_meta for existing key
        storage.put_value("test_key", Bytes::from_static(b"value"), true).await?;
        storage.put_meta("test_key", meta.clone()).await?;
        let m = storage.get_meta("test_key").await?;
        assert_eq!(m, Some(meta.to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_meta() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // get_meta for non-existent key
        let m = storage.get_meta("non_existent").await?;
        assert_eq!(m, None);

        // get_meta for existing key with meta
        storage
            .put_value_with_meta("name_with_meta", Bytes::from_static(b"value_with_meta"), Bytes::from_static(b"meta1"), true)
            .await?;
        let m = storage.get_meta("name_with_meta").await?;
        assert_eq!(m, Some(b"meta1".to_vec()));

        // get_meta for existing key without meta
        storage.put_value("name_without_meta", Bytes::from_static(b"value"), true).await?;
        let m = storage.get_meta("name_without_meta").await?;
        assert_eq!(m, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // delete non-existent key (should not error)
        storage.delete("non_existent").await?;

        // delete existing key
        storage.put_value("to_delete", Bytes::from_static(b"del"), true).await?;
        assert_eq!(storage.get_value("to_delete").await?, Some(b"del".to_vec()));
        storage.delete("to_delete").await?;
        assert_eq!(storage.get_value("to_delete").await?, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_bulk() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // delete_bulk with non-existent keys (should not error)
        storage.delete_bulk(&["non_existent1", "non_existent2"]).await?;

        // delete_bulk with existing keys
        storage.put_value("bulk1", Bytes::from_static(b"b1"), true).await?;
        storage.put_value("bulk2", Bytes::from_static(b"b2"), true).await?;
        storage.put_value("bulk3", Bytes::from_static(b"b3"), true).await?;
        assert_eq!(storage.get_value("bulk1").await?, Some(b"b1".to_vec()));
        assert_eq!(storage.get_value("bulk2").await?, Some(b"b2".to_vec()));
        assert_eq!(storage.get_value("bulk3").await?, Some(b"b3".to_vec()));

        storage.delete_bulk(&["bulk1", "bulk2"]).await?;
        assert_eq!(storage.get_value("bulk1").await?, None);
        assert_eq!(storage.get_value("bulk2").await?, None);
        assert_eq!(storage.get_value("bulk3").await?, Some(b"b3".to_vec()));

        Ok(())
    }

    #[tokio::test]
    async fn test_shrink() -> TestResult<()> {
        let (_temp_dir, storage) = create_test_storage().await?;

        // Add keys
        storage.put_value("keep1", Bytes::from_static(b"k1"), true).await?;
        storage.put_value("keep2", Bytes::from_static(b"k2"), true).await?;
        storage.put_value("drop1", Bytes::from_static(b"d1"), true).await?;
        storage.put_value("drop2", Bytes::from_static(b"d2"), true).await?;

        // Check initial state
        let names_before: Vec<Box<[u8]>> = storage.get_keys()?.collect();
        assert_eq!(names_before.len(), 4);

        // shrink keep only keys starting with "keep"
        storage.shrink(|k| k.starts_with(b"keep")).await?;

        // Check final state
        let names_after: Vec<Box<[u8]>> = storage.get_keys()?.collect();
        let names_after_vec: Vec<Vec<u8>> = names_after.into_iter().map(|b| b.into_vec()).collect();
        assert_eq!(names_after_vec.len(), 2);
        assert!(names_after_vec.iter().any(|n| n == b"keep1"));
        assert!(names_after_vec.iter().any(|n| n == b"keep2"));
        assert!(!names_after_vec.iter().any(|n| n == b"drop1"));
        assert!(!names_after_vec.iter().any(|n| n == b"drop2"));

        Ok(())
    }
}
