use async_trait::async_trait;
use tokio_util::bytes::Bytes;

use omnius_core_omnikit::service::connection::codec::{FramedRecv, FramedSend};

use crate::prelude::*;
use crate::protocol::MessageCodec;

#[async_trait]
pub trait FramedRecvExt: FramedRecv {
    async fn recv_message<T: RocketPackStruct>(&mut self) -> Result<T>;
    async fn recv_message_with<C: MessageCodec>(&mut self) -> Result<C::Message>;
}

#[async_trait]
impl<T: FramedRecv> FramedRecvExt for T
where
    T: ?Sized + Send + Unpin,
{
    async fn recv_message<TItem: RocketPackStruct>(&mut self) -> Result<TItem> {
        let b = self.recv().await?;
        let item = TItem::import(&b)?;
        Ok(item)
    }

    async fn recv_message_with<C: MessageCodec>(&mut self) -> Result<C::Message> {
        Ok(C::decode(&self.recv().await?)?)
    }
}

#[async_trait]
pub trait FramedSendExt: FramedSend {
    async fn send_message<T: RocketPackStruct + Send + Sync>(&mut self, item: &T) -> Result<()>;
    async fn send_message_with<C: MessageCodec>(&mut self, item: &C::Message) -> Result<()>
    where
        C::Message: Send + Sync;
}

#[async_trait]
impl<T: FramedSend> FramedSendExt for T
where
    T: ?Sized + Send + Unpin,
{
    async fn send_message<TItem: RocketPackStruct + Send + Sync>(&mut self, item: &TItem) -> Result<()> {
        let b = Bytes::from(item.export()?);
        self.send(b).await?;
        Ok(())
    }

    async fn send_message_with<C: MessageCodec>(&mut self, item: &C::Message) -> Result<()>
    where
        C::Message: Send + Sync,
    {
        self.send(Bytes::from(C::encode(item)?)).await?;
        Ok(())
    }
}
