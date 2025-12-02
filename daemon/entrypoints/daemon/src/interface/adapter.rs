use tokio::io::{AsyncRead, AsyncWrite};

use omnius_core_omnikit::service::remoting::{OmniRemotingListener, OmniRemotingStream};

use crate::prelude::*;

mod error;
mod request;
mod result;

pub use error::*;
pub use request::*;
pub use result::*;

pub struct OmniRemotingListenerAdapter<R, W>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    listener: OmniRemotingListener<R, W>,
}

impl<R, W> OmniRemotingListenerAdapter<R, W>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(listener: OmniRemotingListener<R, W>) -> Self {
        Self { listener }
    }

    pub fn function_id(&self) -> u32 {
        self.listener.function_id()
    }

    pub async fn listen_stream<F>(&self, callback: F) -> Result<()>
    where
        F: AsyncFnOnce(OmniRemotingStreamAdapter<R, W>),
    {
        self.listener
            .listen_stream(|stream| {
                let stream_adapter = OmniRemotingStreamAdapter::new(stream);
                callback(stream_adapter)
            })
            .await?;
        Ok(())
    }

    pub async fn listen_unary<TRequestMessage, TResultMessage, F>(&self, callback: F) -> Result<()>
    where
        TRequestMessage: RocketPackStruct + std::fmt::Debug + Send + Sync + 'static,
        TResultMessage: RocketPackStruct + std::fmt::Debug + Send + Sync + 'static,
        F: AsyncFnOnce(ApiRequest<TRequestMessage>) -> ApiResult<TResultMessage>,
    {
        self.listen_stream(async |stream| {
            let request = match stream.recv_request::<TRequestMessage>().await {
                Ok(input) => input,
                Err(_) => {
                    let _ = stream.send_result(ApiResult::<TResultMessage>::Err(ApiError::new(ApiErrorKind::InvalidInput))).await;
                    return;
                }
            };

            let result = callback(request).await;
            let _ = stream.send_result(result).await;
        })
        .await?;
        Ok(())
    }
}

pub struct OmniRemotingStreamAdapter<R, W>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    stream: OmniRemotingStream<R, W>,
}

impl<R, W> OmniRemotingStreamAdapter<R, W>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(stream: OmniRemotingStream<R, W>) -> Self {
        Self { stream }
    }

    pub async fn send_result<T>(&self, message: ApiResult<T>) -> Result<()>
    where
        T: RocketPackStruct + std::fmt::Debug + Send + Sync + 'static,
    {
        trace!(
            request_type = std::any::type_name::<T>(),
            result = ?message,
            "rpc send request"
        );
        self.stream.send(message).await?;
        Ok(())
    }

    pub async fn recv_request<T>(&self) -> Result<ApiRequest<T>>
    where
        T: RocketPackStruct + std::fmt::Debug + Send + Sync + 'static,
    {
        let message = self.stream.recv::<ApiRequest<T>>().await?;
        trace!(
            request_type = std::any::type_name::<T>(),
            header = ?message.header,
            body = ?message.body,
            "rpc recv request"
        );
        Ok(message)
    }
}
