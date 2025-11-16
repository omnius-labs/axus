use tokio::net::TcpListener;
use tracing::warn;

use omnius_core_omnikit::service::remoting::OmniRemotingListener;

use crate::{
    interface::{adapter::OmniRemotingListenerAdapter, v1},
    prelude::*,
    shared::AppState,
};

#[repr(u32)]
#[derive(Debug, Clone, strum::FromRepr)]
enum FunctionId {
    Health = 0,

    Other = 1,
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
        state.engine.as_ref().shutdown().await;
    }

    pub async fn run(state: &AppState) -> Result<()> {
        let tcp_listener = TcpListener::bind(state.conf.listen_addr.clone()).await?;
        info!(addr = state.conf.listen_addr, "listen");

        loop {
            let (tcp_stream, _) = tcp_listener.accept().await?;
            let (reader, writer) = tokio::io::split(tcp_stream);

            let remoting_listener = OmniRemotingListenerAdapter::new(OmniRemotingListener::<_, _>::new(reader, writer, 1024 * 1024).await?);

            let function_id = remoting_listener.function_id();
            let Some(function_id) = FunctionId::from_repr(function_id) else {
                warn!("unknown function id: {}", function_id);
                continue;
            };

            match function_id {
                FunctionId::Health => remoting_listener.listen_unary(async |param| v1::features::health(state, param).await).await?,
                _ => warn!("not supported: {:?}", function_id),
            }
        }
    }
}
