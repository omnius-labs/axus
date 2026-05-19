mod accepter;
mod connector;
pub mod message;
pub mod model;

pub use accepter::*;
pub use connector::*;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;

    use omnius_core_base::sleeper::FakeSleeper;
    use omnius_core_omnikit::model::{OmniAddr, OmniSignType, OmniSigner};

    use crate::{
        base::{
            connection::{ConnectionTcpAccepterImpl, ConnectionTcpConnectorImpl, FramedRecvExt as _, FramedSendExt as _, TcpProxyOption, TcpProxyType},
            runtime::Shutdown,
        },
        core::session::{SessionAccepter, SessionConnector, model::SessionType},
        prelude::*,
    };

    #[tokio::test]
    #[ignore]
    async fn simple_test() -> TestResult {
        let tcp_accepter = Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 60000), false).await?);
        let tcp_connector = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );

        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let sleeper = Arc::new(FakeSleeper);

        let session_accepter = SessionAccepter::new(tcp_accepter.clone(), signer.clone(), sleeper.clone(), rng.clone()).await;
        let session_connector = SessionConnector::new(tcp_connector, signer, rng);

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

        session_accepter.shutdown().await;
        tcp_accepter.shutdown().await;

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
            let mut value: Option<String> = None;

            let count = decoder.read_map()?;

            for _ in 0..count {
                match decoder.read_u64()? {
                    0 => value = Some(decoder.read_string()?),
                    _ => decoder.skip_field()?,
                }
            }

            Ok(Self {
                value: value.ok_or(RocketPackDecoderError::Other("missing field: value"))?,
            })
        }
    }
}
