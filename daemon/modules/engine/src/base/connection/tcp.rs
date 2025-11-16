mod accepter;
mod connector;
mod upnp_client;

pub use accepter::*;
pub use connector::*;
pub use upnp_client::*;

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::model::OmniAddr;
    use testresult::TestResult;

    use crate::{
        base::connection::{
            ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnector, ConnectionTcpConnectorImpl, FramedRecvExt as _, FramedSendExt as _, TcpProxyOption,
            TcpProxyType,
        },
        prelude::*,
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

    impl RocketPackStruct for TestMessage {
        fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
            encoder.write_map(1)?;

            encoder.write_u64(0)?;
            encoder.write_string(value.value.as_str())?;

            Ok(())
        }

        fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
        where
            Self: Sized,
        {
            let count = decoder.read_map()?;

            let mut value: Option<String> = None;

            for _ in 0..count {
                match decoder.read_u64()? {
                    0 => value = Some(decoder.read_string()?),
                    _ => decoder.skip_field()?,
                }
            }

            Ok(Self {
                value: value.ok_or(RocketPackDecoderError::Other("missing field: test"))?,
            })
        }
    }
}
