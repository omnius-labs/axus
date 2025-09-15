use std::sync::Arc;

use omnius_core_base::random_bytes::RandomBytesProvider;
use omnius_core_omnikit::model::{OmniAddr, OmniSigner};
use parking_lot::Mutex;

use crate::{
    base::connection::{ConnectionTcpConnector, FramedRecvExt as _, FramedSendExt as _},
    core::session::message::{V1ChallengeMessage, V1SignatureMessage},
    prelude::*,
};

use super::{
    message::{HelloMessage, SessionVersion, V1RequestMessage, V1RequestType, V1ResultMessage, V1ResultType},
    model::{Session, SessionHandshakeType, SessionType},
};

pub struct SessionConnector {
    tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>,
    signer: Arc<OmniSigner>,
    random_bytes_provider: Arc<Mutex<dyn RandomBytesProvider + Send + Sync>>,
}

impl SessionConnector {
    pub fn new(
        tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>,
        signer: Arc<OmniSigner>,
        random_bytes_provider: Arc<Mutex<dyn RandomBytesProvider + Send + Sync>>,
    ) -> Self {
        Self {
            tcp_connector,
            signer,
            random_bytes_provider,
        }
    }

    pub async fn connect(&self, addr: &OmniAddr, typ: &SessionType) -> Result<Session> {
        let stream = self.tcp_connector.connect(addr).await?;

        let send_hello_message = HelloMessage { version: SessionVersion::V1 };
        stream.sender.lock().await.send_message(&send_hello_message).await?;
        let received_hello_message: HelloMessage = stream.receiver.lock().await.recv_message().await?;

        let version = send_hello_message.version | received_hello_message.version;

        if version.contains(SessionVersion::V1) {
            let send_nonce: [u8; 32] = self.random_bytes_provider.lock().get_bytes(32).as_slice().try_into()?;
            let send_challenge_message = V1ChallengeMessage { nonce: send_nonce };
            stream.sender.lock().await.send_message(&send_challenge_message).await?;
            let receive_challenge_message: V1ChallengeMessage = stream.receiver.lock().await.recv_message().await?;

            let send_signature = self.signer.sign(&receive_challenge_message.nonce)?;
            let send_signature_message = V1SignatureMessage { cert: send_signature };
            stream.sender.lock().await.send_message(&send_signature_message).await?;
            let received_signature_message: V1SignatureMessage = stream.receiver.lock().await.recv_message().await?;

            if received_signature_message.cert.verify(send_nonce.as_slice()).is_err() {
                return Err(Error::builder().kind(ErrorKind::InvalidFormat).message("Invalid signature").build());
            }

            let send_session_request_message = V1RequestMessage {
                request_type: match typ {
                    SessionType::NodeFinder => V1RequestType::NodeFinder,
                    SessionType::FileExchanger => V1RequestType::FileExchanger,
                },
            };
            stream.sender.lock().await.send_message(&send_session_request_message).await?;
            let received_session_result_message: V1ResultMessage = stream.receiver.lock().await.recv_message().await?;

            if received_session_result_message.result_type == V1ResultType::Reject {
                return Err(Error::builder().kind(ErrorKind::Reject).message("Session rejected").build());
            }

            let session = Session {
                typ: typ.clone(),
                address: addr.clone(),
                handshake_type: SessionHandshakeType::Connected,
                cert: received_signature_message.cert,
                stream,
            };

            Ok(session)
        } else {
            Err(Error::builder()
                .kind(ErrorKind::UnsupportedVersion)
                .message(format!("Unsupported session version: {version:?}"))
                .build())
        }
    }
}
