use crate::protocol::session::*;
use std::sync::Arc;

use chrono::Utc;
use omnius_core_base::clock::Clock;
use omnius_core_omnikit::generated::omni_sign::OmniSigner;
use omnius_core_omnikit::model::omni_addr::OmniAddr;
use omnius_core_omnikit::service::connection::secure::{OmniSecureStream, OmniSecureStreamType};
use parking_lot::Mutex;
use tokio::{
    net::TcpStream,
    time::{Instant, timeout_at},
};

use crate::{
    base::connection::{ConnectionTcpConnector, FramedRecvExt as _, FramedSendExt as _, FramedStream},
    prelude::*,
};

use super::{
    message::{HelloMessage, SessionVersion, V1RequestMessage, V1RequestType, V1ResultMessage, V1ResultType},
    model::{Session, SessionHandshakeType, SessionOption, SessionType},
};

pub struct SessionConnector {
    tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>,
    signer: Arc<OmniSigner>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
    option: SessionOption,
}

impl SessionConnector {
    pub fn new(
        tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>,
        signer: Arc<OmniSigner>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: SessionOption,
    ) -> Self {
        Self {
            tcp_connector,
            signer,
            clock,
            rng,
            option,
        }
    }

    pub async fn connect(&self, addr: &OmniAddr, typ: &SessionType) -> Result<Session> {
        let stream = self.tcp_connector.connect(addr).await?;
        let deadline = Instant::now() + self.option.handshake_timeout;
        timeout_at(deadline, self.handshake(stream, addr, typ))
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("Session handshake timed out"))?
    }

    async fn handshake(&self, stream: TcpStream, addr: &OmniAddr, typ: &SessionType) -> Result<Session> {
        let stream = OmniSecureStream::new(
            stream,
            OmniSecureStreamType::Connected,
            self.option.handshake_max_frame_length,
            Some(self.signer.as_ref().clone()),
            true,
            self.option.max_clock_skew,
            self.clock.clone(),
            self.rng.clone(),
        )
        .await?;
        let cert = stream
            .peer_cert()
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::InvalidFormat).with_message("Missing peer signature"))?;
        let (reader, writer) = tokio::io::split(stream);
        let stream = FramedStream::new(reader, writer, self.option.handshake_max_frame_length);

        let send_hello_message = HelloMessage { version: SessionVersion::V1 };
        stream.sender.lock().await.send_message_with::<HelloMessageCodec>(&send_hello_message).await?;
        let received_hello_message: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;

        let version = send_hello_message.version & received_hello_message.version;

        if version.contains(SessionVersion::V1) {
            let send_session_request_message = V1RequestMessage {
                request_type: match typ {
                    SessionType::NodeFinder => V1RequestType::NodeFinder,
                    SessionType::FileExchanger => V1RequestType::FileExchanger,
                },
            };
            stream.sender.lock().await.send_message_with::<V1RequestMessageCodec>(&send_session_request_message).await?;
            let received_session_result_message: V1ResultMessage = stream.receiver.lock().await.recv_message_with::<V1ResultMessageCodec>().await?;

            match received_session_result_message.result_type {
                V1ResultType::Accept => {}
                V1ResultType::Reject => return Err(Error::new(ErrorKind::Reject).with_message("Session rejected")),
                V1ResultType::Unknown => return Err(Error::new(ErrorKind::UnsupportedType).with_message("Session use selection result is unknown")),
            }

            stream.set_max_frame_length(typ.max_frame_length()).await;

            let session = Session {
                typ: typ.clone(),
                address: addr.clone(),
                handshake_type: SessionHandshakeType::Connected,
                cert,
                stream,
            };

            Ok(session)
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("Unsupported session version: {}", version.bits())))
        }
    }
}
