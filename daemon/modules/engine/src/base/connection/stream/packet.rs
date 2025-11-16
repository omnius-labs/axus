use async_trait::async_trait;
use omnius_core_omnikit::service::connection::codec::{FramedRecv, FramedSend};

use crate::prelude::*;

#[async_trait]
pub trait FramedRecvExt: FramedRecv {
    async fn recv_message<T: RocketPackStruct>(&mut self) -> Result<T>;
}

#[async_trait]
impl<T: FramedRecv> FramedRecvExt for T
where
    T: ?Sized + Send + Unpin,
{
    async fn recv_message<TItem: RocketPackStruct>(&mut self) -> Result<TItem> {
        let mut b = self.recv().await?;
        let item = TItem::import(&mut b)?;
        Ok(item)
    }
}

#[async_trait]
pub trait FramedSendExt: FramedSend {
    async fn send_message<T: RocketPackStruct + Send + Sync>(&mut self, item: &T) -> Result<()>;
}

#[async_trait]
impl<T: FramedSend> FramedSendExt for T
where
    T: ?Sized + Send + Unpin,
{
    async fn send_message<TItem: RocketPackStruct + Send + Sync>(&mut self, item: &TItem) -> Result<()> {
        let b = item.export()?;
        self.send(b).await?;
        Ok(())
    }
}
