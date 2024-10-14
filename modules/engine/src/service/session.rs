mod accepter;
mod connector;
pub mod message;
pub mod model;

pub use accepter::*;
pub use connector::*;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use omnius_core_base::{random_bytes::RandomBytesProviderImpl, sleeper::FakeSleeper};
    use omnius_core_omnikit::{OmniAddr, OmniSignType, OmniSigner};
    use omnius_core_rocketpack::{RocketMessage, RocketMessageReader, RocketMessageWriter};
    use parking_lot::Mutex;
    use testresult::TestResult;

    use crate::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        service::{
            connection::{
                ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnector, ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType,
            },
            session::{model::SessionType, SessionAccepter, SessionConnector},
        },
    };

    #[tokio::test]
    #[ignore]
    async fn simple_test() -> TestResult {
        let tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync> =
            Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 60000), false).await?);
        let tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync> = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );

        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let random_bytes_provider = Arc::new(Mutex::new(RandomBytesProviderImpl::new()));
        let sleeper = Arc::new(FakeSleeper);

        let session_accepter = SessionAccepter::new(tcp_accepter.clone(), signer.clone(), random_bytes_provider.clone(), sleeper.clone()).await;
        let session_connector = SessionConnector::new(tcp_connector, signer, random_bytes_provider);

        let client = Arc::new(
            session_connector
                .connect(&OmniAddr::create_tcp("127.0.0.1".parse()?, 60000), &SessionType::NodeFinder)
                .await?,
        );
        let server = Arc::new(session_accepter.accept(&SessionType::NodeFinder).await?);

        client
            .stream
            .sender
            .lock()
            .await
            .send_message(&TestMessage {
                value: "Hello, World!".to_string(),
            })
            .await?;
        let text: TestMessage = server.stream.receiver.lock().await.recv_message().await?;

        println!("{}", text.value);

        session_accepter.terminate().await?;
        tcp_accepter.terminate().await?;

        Ok(())
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TestMessage {
        pub value: String,
    }

    impl RocketMessage for TestMessage {
        fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> anyhow::Result<()> {
            writer.write_str(&value.value);

            Ok(())
        }

        fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> anyhow::Result<Self>
        where
            Self: Sized,
        {
            let value = reader.get_string(1024)?;

            Ok(Self { value })
        }
    }
}
