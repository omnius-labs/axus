use tokio::net::TcpListener;

use omnius_core_omnikit::service::remoting::{OmniRemotingDefaultErrorMessage, OmniRemotingListener};
use tracing::warn;

use crate::{prelude::*, shared::AppState};

use super::features;

#[derive(Debug, Clone, strum::FromRepr)]
enum FunctionId {
    Health,

    ConfigGet,
    ConfigSet,

    PublishedFile,

    SubscribedFile,

    SubscribedFileGet,
}
pub struct RpcServer;

impl RpcServer {
    pub async fn serve(state: AppState) -> Result<()> {
        tokio::select! {
            Err(e) = Self::run(&state) => {
                error!(error = ?e)
            },
            _ = tokio::signal::ctrl_c() => {
               Self::shutdown(&state).await;
            }
        }
        Ok(())
    }

    async fn shutdown(state: &AppState) {
        state.engine.shutdown().await;
    }

    async fn run(state: &AppState) -> Result<()> {
        let tcp_listener = TcpListener::bind(state.conf.listen_addr.clone()).await?;
        info!(addr = state.conf.listen_addr, "listen");

        loop {
            let (tcp_stream, _) = tcp_listener.accept().await?;
            let (reader, writer) = tokio::io::split(tcp_stream);

            let mut remoting_listener = OmniRemotingListener::<_, _, OmniRemotingDefaultErrorMessage>::new(reader, writer, 1024 * 1024);
            remoting_listener.handshake().await?;

            let function_id = remoting_listener.function_id()?;
            let Some(function_id) = FunctionId::from_repr(function_id as usize) else {
                warn!("unknown function id: {}", function_id);
                continue;
            };

            match function_id {
                FunctionId::Health => remoting_listener.listen_unary(async |p| features::health(state, p).await).await?,
                _ => warn!("not supported"),
            }
        }
    }
}
