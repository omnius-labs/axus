use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::Mutex as TokioMutex;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};

use crate::{
    core::{storage::KeyValueFileStorage, util::Terminable},
    prelude::*,
};

use super::*;

#[allow(unused)]
pub struct FilePublisher {
    file_publisher_repo: Arc<FilePublisherRepo>,
    blocks_storage: Arc<KeyValueFileStorage>,

    task_encoder: Arc<TokioMutex<Option<Arc<TaskEncoder>>>>,

    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
}

#[async_trait]
impl Terminable for FilePublisher {
    async fn terminate(&self) {
        {
            let mut task_encoder = self.task_encoder.lock().await;
            if let Some(task_encoder) = task_encoder.take() {
                task_encoder.terminate().await;
            }
        }
    }
}

#[allow(unused)]
impl FilePublisher {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        file_publisher_repo: Arc<FilePublisherRepo>,
        blocks_storage: Arc<KeyValueFileStorage>,

        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            file_publisher_repo,
            blocks_storage,

            task_encoder: Arc::new(TokioMutex::new(None)),

            tsid_provider,
            clock,
            sleeper,
        });
        v.start().await?;

        Ok(v)
    }

    async fn start(&self) -> Result<()> {
        let task = TaskEncoder::new(
            self.file_publisher_repo.clone(),
            self.blocks_storage.clone(),
            self.tsid_provider.clone(),
            self.clock.clone(),
            self.sleeper.clone(),
        )
        .await?;
        self.task_encoder.lock().await.replace(task);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    #[tokio::test]
    pub async fn simple_test() -> TestResult {
        Ok(())
    }
}
