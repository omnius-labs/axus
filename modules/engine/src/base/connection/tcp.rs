mod accepter;
mod connector;
mod upnp_client;

pub use accepter::*;
pub use connector::*;
pub use upnp_client::*;

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::model::OmniAddr;
    use omnius_core_rocketpack::{Result as RocketPackResult, RocketMessage, RocketMessageReader, RocketMessageWriter};
    use testresult::TestResult;

    use crate::base::connection::{
        ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnector, ConnectionTcpConnectorImpl, FramedRecvExt as _, FramedSendExt as _, TcpProxyOption, TcpProxyType,
    };

    #[tokio::test]
    #[ignore]
    async fn simple_test() -> TestResult {
        let accepter = ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 50000), false).await?;
        let connector = ConnectionTcpConnectorImpl::new(TcpProxyOption {
            typ: TcpProxyType::None,
            addr: None,
        })
        .await?;

        let connected_stream = connector.connect(&OmniAddr::new("tcp(ip4(127.0.0.1),50000)")).await?;
        let (accepted_stream, _) = accepter.accept().await?;

        connected_stream
            .sender
            .lock()
            .await
            .send_message(&TestMessage {
                value: "Hello, World!".to_string(),
            })
            .await?;
        let text: TestMessage = accepted_stream.receiver.lock().await.recv_message().await?;

        println!("{}", text.value);

        Ok(())
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TestMessage {
        pub value: String,
    }

    impl RocketMessage for TestMessage {
        fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
            writer.put_str(&value.value);

            Ok(())
        }

        fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
        where
            Self: Sized,
        {
            let value = reader.get_string(1024)?;

            Ok(Self { value })
        }
    }
}
