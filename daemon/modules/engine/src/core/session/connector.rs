use std::sync::Arc;

use omnius_core_omnikit::{
    generated::omni_sign::OmniSigner,
    model::omni_addr::OmniAddr,
    service::connection::secure::{OmniSecureAuth, OmniSecureStream, OmniSecureStreamType},
};
use parking_lot::Mutex;
use rand_core::CryptoRng;
use tokio::time::{Instant, timeout_at};

use super::{
    message::{HelloMessage, SessionVersion, V2RequestMessage, V2RequestType, V2ResultMessage, V2ResultType},
    model::{Session, SessionHandshakeType, SessionOption, SessionType},
};
use crate::{
    base::connection::{ConnectionTcpConnector, FramedRecvExt as _, FramedSendExt as _, FramedStream, RawStream},
    prelude::*,
    protocol::session::*,
};

pub struct SessionConnector {
    tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>,
    signer: Arc<OmniSigner>,
    rng: Arc<Mutex<dyn CryptoRng + Send + Sync>>,
    option: SessionOption,
}

impl SessionConnector {
    pub fn new(tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>, signer: Arc<OmniSigner>, rng: Arc<Mutex<dyn CryptoRng + Send + Sync>>, option: SessionOption) -> Self {
        Self {
            tcp_connector,
            signer,
            rng,
            option,
        }
    }

    pub async fn connect(&self, addr: &OmniAddr, typ: &SessionType, expected_public_key: &[u8]) -> Result<Session> {
        let stream = self.tcp_connector.connect(addr).await?;
        let deadline = Instant::now() + self.option.handshake_timeout;
        timeout_at(deadline, self.handshake(stream, addr, typ, expected_public_key))
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("Session handshake timed out"))?
    }

    async fn handshake(&self, raw: RawStream, addr: &OmniAddr, typ: &SessionType, expected_public_key: &[u8]) -> Result<Session> {
        let secure = OmniSecureStream::new(
            raw,
            OmniSecureStreamType::Connected,
            self.option.secure_option()?,
            OmniSecureAuth::Mutual { signer: self.signer.clone() },
            self.rng.clone(),
        )
        .await?;
        let cert = secure
            .peer_cert()
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::Reject).with_message("missing peer identity"))?;
        if cert.public_key != expected_public_key {
            return Err(Error::new(ErrorKind::Reject).with_message("connected identity differs from expected public key"));
        }
        // 公開鍵の照合後にだけ、暗号化した application frame を送受信する。
        let stream = FramedStream::from_stream(secure, self.option.handshake_max_frame_length);
        stream
            .sender
            .lock()
            .await
            .send_message_with::<HelloMessageCodec>(&HelloMessage { version: SessionVersion::V2 })
            .await?;
        let hello: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        if hello.version != SessionVersion::V2 {
            return Err(Error::new(ErrorKind::UnsupportedType).with_message("unsupported session version"));
        }
        let request_type = match typ {
            SessionType::NodeFinder => V2RequestType::NodeFinder,
            SessionType::FileExchanger => V2RequestType::FileExchanger,
        };
        stream
            .sender
            .lock()
            .await
            .send_message_with::<V2RequestMessageCodec>(&V2RequestMessage { request_type })
            .await?;
        let result: V2ResultMessage = stream.receiver.lock().await.recv_message_with::<V2ResultMessageCodec>().await?;
        match result.result_type {
            V2ResultType::Accept => {}
            V2ResultType::Reject => return Err(Error::new(ErrorKind::Reject).with_message("Session rejected")),
            V2ResultType::Unknown => return Err(Error::new(ErrorKind::InvalidFormat).with_message("invalid Session result")),
        }
        stream.set_max_frame_length(typ.max_frame_length()).await;
        Ok(Session {
            typ: typ.clone(),
            address: addr.clone(),
            handshake_type: SessionHandshakeType::Connected,
            cert,
            stream,
        })
    }
}
