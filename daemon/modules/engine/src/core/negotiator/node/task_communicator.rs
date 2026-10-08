use crate::protocol::{
    MessageCodec,
    node::{DataMessageCodec, HelloMessageCodec, ProfileMessageCodec},
};
use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use enumflags2::make_bitflags;
use parking_lot::Mutex;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, mpsc},
    task::JoinHandle,
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;

use omnius_core_base::sleeper::Sleeper;

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::session::model::Session,
    model::NodeProfile,
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskCommunicator {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    sessions: Arc<SessionRegistry>,
    node_profile_repo: Arc<NodeFinderRepo>,
    session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    communicate_join_handles: Arc<TokioMutex<Vec<JoinHandle<()>>>>,
    cancellation_token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskCommunicator {
    async fn shutdown(&self) {
        self.cancellation_token.cancel();

        if let Some(join_handle) = self.join_handle.lock().await.take() {
            let _ = join_handle.await;
        }

        for join_handle in self.communicate_join_handles.lock().await.drain(..) {
            let _ = join_handle.await;
        }
    }
}

impl TaskCommunicator {
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        sessions: Arc<SessionRegistry>,
        node_profile_repo: Arc<NodeFinderRepo>,
        session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let cancellation_token = CancellationToken::new();

        let v = Arc::new(Self {
            my_node_profile,
            sessions,
            node_profile_repo,
            session_receiver,
            sleeper,
            option,
            join_handle: Arc::new(TokioMutex::new(None)),
            communicate_join_handles: Arc::new(TokioMutex::new(Vec::new())),
            cancellation_token: cancellation_token.clone(),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            while !this.cancellation_token.is_cancelled() {
                // 終了済みのタスクを削除
                this.communicate_join_handles.lock().await.retain(|join_handle| !join_handle.is_finished());

                let status = select! {
                    status = async { this.session_receiver.lock().await.recv().await } => status,
                    _ = this.cancellation_token.cancelled() => break,
                };
                if let Some(status) = status {
                    let communicator = this.clone();
                    let join_handle = tokio::spawn(async move {
                        let res = communicator.communicate(status).await;
                        if let Err(e) = res {
                            warn!(error_message = e.to_string(), "communicate failed");
                        }
                    });
                    this.communicate_join_handles.lock().await.push(join_handle);
                }
            }
        }));

        Ok(())
    }

    async fn communicate(self: Arc<Self>, status: SessionStatus) -> Result<()> {
        let my_node_profile = self.my_node_profile.lock().clone();
        // 登録前なので、cancel されたら後始末なしで終える
        let (other_node_profile, received_at) = select! {
            res = Self::handshake(&status.session, &my_node_profile, self.option.intervals.receive_timeout()) => res?,
            _ = self.cancellation_token.cancelled() => return Ok(()),
        };

        let other_node_profile_uri = other_node_profile.to_uri()?;
        *status.node_profile.lock() = Some(other_node_profile.clone());

        let status = Arc::new(status);

        match self.sessions.register(my_node_profile.id(), other_node_profile.id(), &status).await {
            Registration::Added => {}
            Registration::Replaced => info!(node_profile = other_node_profile_uri, "Session replaced"),
            Registration::Rejected => return Err(Error::new(ErrorKind::AlreadyExists).with_message("Session already exists")),
        }

        info!(node_profile = other_node_profile_uri, "Session established");

        let s = self.clone().send(status.clone()).await;
        let r = self.clone().receive(status.clone(), received_at).await;
        let _ = tokio::join!(s, r);

        info!(node_profile = other_node_profile_uri, "Session closed");

        self.sessions.unregister(other_node_profile.id(), &status).await;

        Ok(())
    }

    pub async fn handshake(session: &Session, node_profile: &NodeProfile, receive_timeout: Duration) -> Result<(NodeProfile, Instant)> {
        let deadline = Instant::now() + receive_timeout;
        let send_hello_message = HelloMessage {
            version: make_bitflags!(NodeFinderVersion::V1),
        };
        let (received_hello_message, received_at) = Self::exchange_message::<HelloMessageCodec>(session, &send_hello_message, deadline).await?;

        let version = send_hello_message.version & received_hello_message.version;

        if version.contains(NodeFinderVersion::V1) {
            let send_profile_message = ProfileMessage {
                node_profile: node_profile.clone(),
            };
            let (received_profile_message, received_at) = Self::exchange_message::<ProfileMessageCodec>(session, &send_profile_message, received_at + receive_timeout).await?;

            if received_profile_message.node_profile.id() == node_profile.id() {
                return Err(Error::new(ErrorKind::Reject).with_message("connected to self"));
            }

            // node ID は公開鍵から導出するため、Session で署名を確かめた鍵と一致すれば ID も相手のものと確定する
            if received_profile_message.node_profile.public_key() != session.cert.public_key.as_slice() {
                return Err(Error::new(ErrorKind::Reject).with_message("node profile does not match the session certificate"));
            }

            Ok((received_profile_message.node_profile, received_at))
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("invalid version: {}", version.bits())))
        }
    }

    async fn exchange_message<C: MessageCodec>(session: &Session, message: &C::Message, deadline: Instant) -> Result<(C::Message, Instant)>
    where
        C::Message: Send + Sync,
    {
        let received = timeout_at(deadline, async {
            session.stream.sender.lock().await.send_message_with::<C>(message).await?;
            session.stream.receiver.lock().await.recv_message_with::<C>().await
        })
        .await
        .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
        Ok((received, Instant::now()))
    }

    async fn send(self: Arc<Self>, status: Arc<SessionStatus>) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let sender = TaskSender { status };
            let f = async {
                loop {
                    this.sleeper.sleep(this.option.intervals.communicate).await;
                    let res = sender.send().await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "send failed",);
                        return;
                    }
                }
            };
            select! {
                _ = f => {}
                _ = this.cancellation_token.cancelled() => {}
                _ = sender.status.cancellation_token.cancelled() => {}
            };
            sender.status.cancellation_token.cancel();
        })
    }

    async fn receive(self: Arc<Self>, status: Arc<SessionStatus>, received_at: Instant) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let receiver = TaskReceiver {
                status,
                node_profile_repo: this.node_profile_repo.clone(),
            };
            let f = async {
                let communicate = this.option.intervals.communicate;
                let receive_timeout = this.option.intervals.receive_timeout();
                let mut deadline = received_at + receive_timeout;
                loop {
                    let res = async {
                        let data_message = timeout_at(deadline, receiver.receive())
                            .await
                            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
                        // 受信した時点で更新し、保存処理にかかった時間で期限を延ばさない
                        let received_at = Instant::now();
                        deadline = received_at + receive_timeout;
                        timeout_at(deadline, receiver.apply(data_message))
                            .await
                            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
                        // 周期より速く送る相手に、保存処理を繰り返させない
                        this.sleeper.sleep(communicate.saturating_sub(received_at.elapsed())).await;
                        Ok::<(), Error>(())
                    }
                    .await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "receive failed",);
                        return;
                    }
                }
            };
            select! {
                _ = f => {}
                _ = this.cancellation_token.cancelled() => {}
                _ = receiver.status.cancellation_token.cancelled() => {}
            }
            receiver.status.cancellation_token.cancel();
        })
    }
}

struct TaskSender {
    status: Arc<SessionStatus>,
}

impl TaskSender {
    async fn send(&self) -> Result<()> {
        let data_message = {
            let mut sending_data_message = self.status.sending_data_message.lock();
            DataMessage {
                push_node_profiles: sending_data_message.push_node_profiles.drain(..).collect(),
                want_asset_keys: sending_data_message.want_asset_keys.drain(..).collect(),
                give_asset_key_locations: sending_data_message.give_asset_key_locations.drain().collect(),
                push_asset_key_locations: sending_data_message.push_asset_key_locations.drain().collect(),
            }
        };

        self.status.session.stream.sender.lock().await.send_message_with::<DataMessageCodec>(&data_message).await?;

        Ok(())
    }
}

struct TaskReceiver {
    status: Arc<SessionStatus>,
    node_profile_repo: Arc<NodeFinderRepo>,
}

impl TaskReceiver {
    async fn receive(&self) -> Result<DataMessage> {
        self.status.session.stream.receiver.lock().await.recv_message_with::<DataMessageCodec>().await
    }

    async fn apply(&self, data_message: DataMessage) -> Result<()> {
        let push_node_profiles: Vec<&NodeProfile> = data_message.push_node_profiles.iter().map(|n| n.as_ref()).collect();
        self.node_profile_repo.insert_or_ignore_node_profiles(&push_node_profiles, 0).await?;
        self.node_profile_repo.shrink(1024).await?;

        {
            let mut received_data_message = self.status.received_data_message.lock();
            received_data_message.want_asset_keys.extend(data_message.want_asset_keys);
            received_data_message.give_asset_key_locations.extend(data_message.give_asset_key_locations);
            received_data_message.push_asset_key_locations.extend(data_message.push_asset_key_locations);

            received_data_message.want_asset_keys.shrink(1024 * 256);
            received_data_message.give_asset_key_locations.shrink(1024 * 256);
            received_data_message.push_asset_key_locations.shrink(1024 * 256);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use chrono::Utc;
    use enumflags2::{BitFlags, make_bitflags};
    use parking_lot::Mutex;
    use testresult::TestResult;
    use tokio::{
        sync::{Mutex as TokioMutex, mpsc},
        time::Instant,
    };
    use tokio_util::bytes::Bytes;

    use omnius_core_base::{
        clock::{Clock, ClockUtc},
        sleeper::{Sleeper, SleeperImpl},
    };
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::{
            connection::{FramedRecvExt as _, FramedSendExt as _, FramedStream},
            runtime::Shutdown as _,
        },
        core::session::model::{Session, SessionHandshakeType, SessionType},
        model::{AssetKey, NodeProfile},
        prelude::*,
        protocol::{
            MessageCodec,
            node::{DataMessageCodec, HelloMessageCodec, ProfileMessageCodec},
        },
    };

    use super::{
        DataMessage, HelloMessage, NodeFinderIntervals, NodeFinderOption, NodeFinderRepo, NodeFinderVersion, ProfileMessage, SessionRegistry, SessionStatus, TaskCommunicator,
    };

    const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn shutdown_completes_while_waiting_for_a_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (task, _session_sender) = create_task_communicator(dir.path().to_str().unwrap(), Arc::new(SessionRegistry::new())).await?;

        assert!(tokio::time::timeout(SHUTDOWN_TIMEOUT, task.shutdown()).await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn shutdown_completes_during_handshake() -> TestResult {
        let dir = tempfile::tempdir()?;
        let sessions = Arc::new(SessionRegistry::new());
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), sessions.clone()).await?;

        // 相手が応答しないので、handshake の待機に入ったまま止まる
        let (session, _peer, _) = session_pair()?;
        session_sender.send(SessionStatus::new(session, Arc::new(ClockUtc))).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(tokio::time::timeout(SHUTDOWN_TIMEOUT, task.shutdown()).await.is_ok());
        assert_eq!(sessions.len().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn shutdown_removes_the_registered_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let sessions = Arc::new(SessionRegistry::new());
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), sessions.clone()).await?;

        // peer を保持し続けることで、Session は相手の切断では終わらず shutdown まで残る
        let (session, peer, peer_node_profile) = session_pair()?;
        let peer_task = tokio::spawn(answer_handshake(peer.clone(), peer_node_profile));
        session_sender.send(SessionStatus::new(session, Arc::new(ClockUtc))).await?;
        peer_task.await??;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            while sessions.len().await == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;

        assert!(tokio::time::timeout(SHUTDOWN_TIMEOUT, task.shutdown()).await.is_ok());
        assert_eq!(sessions.len().await, 0);
        drop(peer);

        Ok(())
    }

    #[tokio::test]
    async fn oversized_node_finder_frame_closes_both_workers_and_removes_the_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let sessions = Arc::new(SessionRegistry::new());
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_millis(10),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator_with_intervals(dir.path().to_str().unwrap(), sessions.clone(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session, Arc::new(ClockUtc))).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            while sessions.len().await == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;

        // 送信側だけが動き続けないことを、相手からの EOF と登録解除で確かめる
        let send = async { peer.sender.lock().await.send(Bytes::from(vec![0; FramedStream::NODE_FINDER_MAX_FRAME_LENGTH + 1])).await };
        let receive = async {
            loop {
                if peer.receiver.lock().await.recv().await.is_err() {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, async { tokio::join!(send, receive) }).await?;
        assert_eq!(sessions.len().await, 0);
        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn silent_session_closes_after_three_communicate_intervals() -> TestResult {
        for interval in [Duration::from_millis(50), Duration::from_millis(100)] {
            let dir = tempfile::tempdir()?;
            let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
            fixture.answer_handshake().await?;
            let start = Instant::now();
            fixture.wait_registered().await?;
            let count = fixture.wait_closed(interval * 4).await?;
            assert!(start.elapsed() >= interval * 2);
            // 受信がなくても、期限が来るまでは空の DataMessage を毎周期送る
            assert!(count >= 2);
            fixture.task.shutdown().await;
        }
        Ok(())
    }

    #[tokio::test]
    async fn data_received_before_the_deadline_keeps_the_session_open_and_resets_the_timeout() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(50);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        fixture.answer_handshake().await?;
        fixture.wait_registered().await?;

        // 3 周期より長く交換し、受信のたびに期限が更新されることを確かめる
        for _ in 0..6 {
            fixture.peer.sender.lock().await.send_message_with::<DataMessageCodec>(&DataMessage::default()).await?;
            let received: DataMessage = tokio::time::timeout(interval * 2, fixture.peer.receiver.lock().await.recv_message_with::<DataMessageCodec>()).await??;
            assert_eq!(received, DataMessage::default());
            assert_eq!(fixture.sessions.len().await, 1);
        }
        fixture.wait_closed(interval * 4).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn receive_waits_for_the_rest_of_the_interval_after_each_message() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(400);
        let intervals = NodeFinderIntervals {
            communicate: interval,
            ..NodeFinderIntervals::default()
        };
        let sleeper = Arc::new(RecordingSleeper {
            requested: Mutex::new(Vec::new()),
        });
        let sessions = Arc::new(SessionRegistry::new());
        let (task, session_sender) = create_task_communicator_with_sleeper(dir.path().to_str().unwrap(), sessions.clone(), node_profile("me"), intervals, sleeper.clone()).await?;
        let (session, peer, node_profile) = session_pair()?;
        session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
        session_sender
            .send(SessionStatus::new(session, Arc::new(ClockUtc)))
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?;
        answer_handshake(peer.clone(), node_profile).await?;

        peer.sender.lock().await.send_message_with::<DataMessageCodec>(&DataMessage::default()).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            // 送信側は周期ちょうどを、受信側は周期から処理にかかった分を引いた時間を要求する
            while !sleeper.requested.lock().iter().any(|d| *d > interval / 2 && *d < interval) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn silent_hello_exchange_closes_on_the_receive_deadline() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(50);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>()).await??;
        fixture.wait_closed(interval * 4).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn profile_exchange_has_a_receive_deadline_that_is_reset_by_hello() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(100);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>()).await??;
        tokio::time::sleep(interval * 2).await;
        fixture.send_hello().await?;
        let _: ProfileMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<ProfileMessageCodec>()).await??;

        // Hello の受信で更新されるので、handshake 開始から 3 周期を過ぎても閉じない
        assert!(tokio::time::timeout(interval * 3 / 2, fixture.peer.receiver.lock().await.recv()).await.is_err());
        fixture.wait_closed(interval * 2).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn profile_received_before_the_deadline_establishes_the_session_and_resets_the_timeout() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(100);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        fixture.send_hello().await?;
        let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>()).await??;
        let _: ProfileMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<ProfileMessageCodec>()).await??;
        tokio::time::sleep(interval * 2).await;
        fixture
            .peer
            .sender
            .lock()
            .await
            .send_message_with::<ProfileMessageCodec>(&ProfileMessage {
                node_profile: fixture.node_profile.clone(),
            })
            .await?;
        fixture.wait_registered().await?;

        // Profile の受信前の期限を引き継ぐと、2 周期目の DataMessage を受け取れない
        for _ in 0..2 {
            let received: DataMessage = tokio::time::timeout(interval * 2, fixture.peer.receiver.lock().await.recv_message_with::<DataMessageCodec>()).await??;
            assert_eq!(received, DataMessage::default());
        }
        fixture.wait_closed(interval * 2).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_cancels_receive_deadlines_in_hello_profile_and_established_phases() -> TestResult {
        for phase in 0..3 {
            let dir = tempfile::tempdir()?;
            let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), Duration::from_secs(3600)).await?;
            let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>()).await??;
            if phase >= 1 {
                fixture.send_hello().await?;
                let _: ProfileMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message_with::<ProfileMessageCodec>()).await??;
            }
            if phase == 2 {
                fixture
                    .peer
                    .sender
                    .lock()
                    .await
                    .send_message_with::<ProfileMessageCodec>(&ProfileMessage {
                        node_profile: fixture.node_profile.clone(),
                    })
                    .await?;
                fixture.wait_registered().await?;
            }
            tokio::time::timeout(Duration::from_millis(300), fixture.task.shutdown()).await?;
            fixture.wait_closed(Duration::from_millis(300)).await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn push_node_profile_count_limit_closes_the_session_on_overflow() -> TestResult {
        check_message_count_limit(0, DataMessage::MAX_PUSH_NODE_PROFILES).await
    }

    #[tokio::test]
    async fn want_asset_key_count_limit_closes_the_session_on_overflow() -> TestResult {
        check_message_count_limit(1, DataMessage::MAX_WANT_ASSET_KEYS).await
    }

    #[tokio::test]
    async fn give_asset_key_location_count_limit_closes_the_session_on_overflow() -> TestResult {
        check_message_count_limit(2, DataMessage::MAX_GIVE_ASSET_KEY_LOCATIONS).await
    }

    #[tokio::test]
    async fn push_asset_key_location_count_limit_closes_the_session_on_overflow() -> TestResult {
        check_message_count_limit(3, DataMessage::MAX_PUSH_ASSET_KEY_LOCATIONS).await
    }

    #[tokio::test]
    async fn location_node_profile_count_limits_close_the_session_on_overflow() -> TestResult {
        for field in [4, 5] {
            check_message_count_limit(field, DataMessage::MAX_LOCATION_NODE_PROFILES).await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn data_message_address_limit_closes_the_session_on_overflow() -> TestResult {
        check_message_count_limit(6, NodeProfile::MAX_WIRE_ADDRS).await
    }

    async fn check_message_count_limit(field: u8, max: usize) -> TestResult {
        let dir = tempfile::tempdir()?;
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), Duration::from_millis(500)).await?;
        fixture.answer_handshake().await?;
        fixture.wait_registered().await?;
        let message = boundary_message(field, max);
        assert_eq!(DataMessageCodec::decode(&DataMessageCodec::encode(&message)?)?, message);
        fixture.peer.sender.lock().await.send_message_with::<DataMessageCodec>(&message).await?;
        // 上限ちょうどの message を受けても、次の周期の送信が続く
        let received: DataMessage = tokio::time::timeout(Duration::from_secs(1), fixture.peer.receiver.lock().await.recv_message_with::<DataMessageCodec>()).await??;
        assert_eq!(received, DataMessage::default());
        assert_eq!(fixture.sessions.len().await, 1);
        let oversized = boundary_message(field, max + 1);
        assert!(DataMessageCodec::encode(&oversized).is_err());
        // 旧 pack は送信上限を検査しないため、不正な peer の受信テストに使う。
        let bytes = crate::protocol::tests::legacy_data(&oversized).export()?;
        fixture.peer.sender.lock().await.send(bytes.into()).await?;
        // 受信期限の 1.5 秒より前に、decode error によって閉じる
        fixture.wait_closed(Duration::from_millis(300)).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    fn boundary_message(field: u8, count: usize) -> DataMessage {
        let profile = Arc::new(node_profile("known"));
        let mut message = DataMessage::default();
        match field {
            0 => message.push_node_profiles = vec![profile; count],
            1 => message.want_asset_keys = (0..count).map(|i| Arc::new(asset_key(i, b"asset"))).collect(),
            2 | 3 => {
                let locations = (0..count).map(|i| (Arc::new(asset_key(i, b"asset")), vec![profile.clone()])).collect();
                if field == 2 {
                    message.give_asset_key_locations = locations;
                } else {
                    message.push_asset_key_locations = locations;
                }
            }
            4 | 5 => {
                let locations = HashMap::from([(Arc::new(asset_key(0, b"asset")), vec![profile; count])]);
                if field == 4 {
                    message.give_asset_key_locations = locations;
                } else {
                    message.push_asset_key_locations = locations;
                }
            }
            6 => {
                message.push_node_profiles = vec![Arc::new(NodeProfile::new(
                    b"known".to_vec(),
                    (0..count).map(|i| OmniAddr::new(format!("addr-{i}"))).collect(),
                ))]
            }
            _ => unreachable!(),
        }
        message
    }

    fn asset_key(i: usize, hash: &[u8]) -> AssetKey {
        AssetKey {
            typ: format!("asset-{i}"),
            hash: omnius_core_omnikit::generated::omni_hash::OmniHash {
                typ: omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType::Sha3_256,
                value: hash.to_vec(),
            },
        }
    }

    #[tokio::test]
    async fn profile_message_address_limit_accepts_eight_and_closes_on_nine() -> TestResult {
        for count in [NodeProfile::MAX_WIRE_ADDRS, NodeProfile::MAX_WIRE_ADDRS + 1] {
            let dir = tempfile::tempdir()?;
            let mut fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), Duration::from_millis(500)).await?;
            fixture.node_profile.addrs = (0..count).map(|i| OmniAddr::new(format!("addr-{i}"))).collect();
            fixture.answer_handshake().await?;
            if count == NodeProfile::MAX_WIRE_ADDRS {
                fixture.wait_registered().await?;
                assert_eq!(fixture.sessions.statuses().await[0].1.node_profile.lock().as_ref().unwrap().addrs.len(), count);
                fixture.task.shutdown().await;
            }
            fixture.wait_closed(Duration::from_millis(300)).await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn large_computed_messages_roundtrip_without_closing_sessions() -> TestResult {
        use super::super::{NodeProfileFetcherMock, ReceivedDataMessage, TaskComputer};
        use crate::base::sync::FnHub;
        use rand::{SeedableRng, rngs::ChaCha20Rng};

        let dir = tempfile::tempdir()?;
        tokio::fs::create_dir_all(dir.path().join("a")).await?;
        tokio::fs::create_dir_all(dir.path().join("b")).await?;
        let (a_session, a_profile, b_session, b_profile) = authenticated_session_pair()?;
        a_session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
        b_session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(1),
            ..NodeFinderIntervals::default()
        };
        let a_sessions = Arc::new(SessionRegistry::new());
        let b_sessions = Arc::new(SessionRegistry::new());
        let (a_task, _a_sender) = create_task_communicator_with_profile(dir.path().join("a").to_str().unwrap(), a_sessions.clone(), a_profile.clone(), intervals.clone()).await?;
        let (b_task, _b_sender) = create_task_communicator_with_profile(dir.path().join("b").to_str().unwrap(), b_sessions.clone(), b_profile.clone(), intervals.clone()).await?;
        _a_sender.send(SessionStatus::new(a_session, Arc::new(ClockUtc))).await?;
        _b_sender.send(SessionStatus::new(b_session, Arc::new(ClockUtc))).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            while a_sessions.len().await != 1 || b_sessions.len().await != 1 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        let a_status = a_sessions.statuses().await[0].1.clone();
        let b_status = b_sessions.statuses().await[0].1.clone();

        // URI 上限内の既知 node を、1 message の件数上限より多く用意する
        let known: Vec<_> = (0..96)
            .map(|i| {
                Arc::new(NodeProfile::new(
                    format!("known-{i}").into_bytes(),
                    (0..NodeProfile::MAX_WIRE_ADDRS).map(|j| OmniAddr::new(format!("addr-{j}"))).collect(),
                ))
            })
            .collect();
        a_task
            .node_profile_repo
            .insert_or_ignore_node_profiles(&known.iter().map(|p| p.as_ref()).collect::<Vec<_>>(), 0)
            .await?;
        let keys: Vec<_> = (0..2048).map(|i| asset_key(i, b_profile.id())).collect();
        {
            // 複数 message 分を集約した候補は、送信時に schema の件数上限へ収める
            let mut received = a_status.received_data_message.lock();
            received.want_asset_keys.extend(keys.iter().cloned().map(Arc::new));
            for key in &keys {
                received.give_asset_key_locations.insert(Arc::new(key.clone()), known[..12].to_vec());
                received.push_asset_key_locations.insert(Arc::new(key.clone()), known[..12].to_vec());
            }
        }
        let want = FnHub::new();
        let push = FnHub::new();
        let _want_handle = want.listener().listen({
            let keys = keys.clone();
            move |_| keys.clone()
        });
        let _push_handle = push.listener().listen({
            let keys = keys.clone();
            move |_| keys.clone()
        });
        let a_computer = TaskComputer::new(
            Arc::new(Mutex::new(a_profile)),
            a_task.node_profile_repo.clone(),
            Arc::new(NodeProfileFetcherMock { node_profiles: vec![] }),
            a_sessions.clone(),
            want.caller(),
            push.caller(),
            Arc::new(SleeperImpl),
            Arc::new(Mutex::new(ChaCha20Rng::seed_from_u64(1))),
            NodeFinderOption {
                state_dir: "".to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
                intervals: intervals.clone(),
            },
        )
        .await?;
        a_computer.compute().await?;
        let sent_counts;
        {
            let sending = a_status.sending_data_message.lock();
            sent_counts = (
                sending.want_asset_keys.len(),
                sending.give_asset_key_locations.len(),
                sending.push_asset_key_locations.len(),
            );
            assert!(!sending.push_node_profiles.is_empty());
            assert!(sent_counts.0 > 0 && sent_counts.1 > 0 && sent_counts.2 > 0);
            assert!(sent_counts.1 < DataMessage::MAX_GIVE_ASSET_KEY_LOCATIONS);
            let message = DataMessage {
                push_node_profiles: sending.push_node_profiles.clone(),
                want_asset_keys: sending.want_asset_keys.clone(),
                give_asset_key_locations: sending.give_asset_key_locations.clone(),
                push_asset_key_locations: sending.push_asset_key_locations.clone(),
            };
            assert!(DataMessageCodec::encode(&message)?.len() <= FramedStream::NODE_FINDER_MAX_FRAME_LENGTH);
            assert!(sending.push_node_profiles.iter().all(|p| p.addrs.len() <= NodeProfile::MAX_WIRE_ADDRS));
            for profiles in sending.give_asset_key_locations.values().chain(sending.push_asset_key_locations.values()) {
                assert_eq!(profiles.len(), DataMessage::MAX_LOCATION_NODE_PROFILES);
                assert!(profiles.iter().all(|p| p.addrs.len() <= NodeProfile::MAX_WIRE_ADDRS));
            }
        }
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            loop {
                let ready = {
                    let data = b_status.received_data_message.lock();
                    data.want_asset_keys.len() == sent_counts.0 && data.give_asset_key_locations.len() == sent_counts.1 && data.push_asset_key_locations.len() == sent_counts.2
                };
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        // 相手の受信状態から回答を計算し、同じ Session で戻す
        *a_status.received_data_message.lock() = ReceivedDataMessage::new(Arc::new(ClockUtc));
        let empty = FnHub::new();
        // 回答側も全 AssetKey を提供し、無作為に選ばれた要求すべてへ回答できるようにする
        let b_push = FnHub::new();
        let _b_push_handle = b_push.listener().listen(move |_| keys.clone());
        let b_computer = TaskComputer::new(
            Arc::new(Mutex::new(b_profile)),
            b_task.node_profile_repo.clone(),
            Arc::new(NodeProfileFetcherMock { node_profiles: vec![] }),
            b_sessions.clone(),
            empty.caller(),
            b_push.caller(),
            Arc::new(SleeperImpl),
            Arc::new(Mutex::new(ChaCha20Rng::seed_from_u64(2))),
            NodeFinderOption {
                state_dir: "".to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
                intervals,
            },
        )
        .await?;
        b_computer.compute().await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            while a_status.received_data_message.lock().give_asset_key_locations.is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        assert_eq!(a_sessions.len().await, 1);
        assert_eq!(b_sessions.len().await, 1);
        a_computer.shutdown().await;
        b_computer.shutdown().await;
        a_task.shutdown().await;
        b_task.shutdown().await;
        Ok(())
    }

    fn authenticated_session_pair() -> Result<(Session, NodeProfile, Session, NodeProfile)> {
        let a_signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "a")?;
        let b_signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "b")?;
        let a_cert = a_signer.sign(b"test")?;
        let b_cert = b_signer.sign(b"test")?;
        let a_profile = NodeProfile::new(a_cert.public_key.clone(), vec![]);
        let b_profile = NodeProfile::new(b_cert.public_key.clone(), vec![]);
        let (a, b) = tokio::io::duplex(64 * 1024);
        let (a_reader, a_writer) = tokio::io::split(a);
        let (b_reader, b_writer) = tokio::io::split(b);
        let a_session = Session {
            typ: SessionType::NodeFinder,
            address: OmniAddr::new("a"),
            handshake_type: SessionHandshakeType::Connected,
            cert: b_cert,
            stream: FramedStream::new(a_reader, a_writer),
        };
        let b_session = Session {
            typ: SessionType::NodeFinder,
            address: OmniAddr::new("b"),
            handshake_type: SessionHandshakeType::Accepted,
            cert: a_cert,
            stream: FramedStream::new(b_reader, b_writer),
        };
        Ok((a_session, a_profile, b_session, b_profile))
    }

    struct CommunicatingSession {
        task: Arc<TaskCommunicator>,
        sessions: Arc<SessionRegistry>,
        _session_sender: mpsc::Sender<SessionStatus>,
        peer: FramedStream,
        node_profile: NodeProfile,
    }

    impl CommunicatingSession {
        async fn new(state_dir: &str, interval: Duration) -> Result<Self> {
            let sessions = Arc::new(SessionRegistry::new());
            let intervals = NodeFinderIntervals {
                communicate: interval,
                ..NodeFinderIntervals::default()
            };
            let (task, session_sender) = create_task_communicator_with_intervals(state_dir, sessions.clone(), intervals).await?;
            let (session, peer, node_profile) = session_pair()?;
            session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
            session_sender
                .send(SessionStatus::new(session, Arc::new(ClockUtc)))
                .await
                .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?;
            Ok(Self {
                task,
                sessions,
                _session_sender: session_sender,
                peer,
                node_profile,
            })
        }

        async fn answer_handshake(&self) -> Result<()> {
            answer_handshake(self.peer.clone(), self.node_profile.clone()).await
        }

        async fn send_hello(&self) -> Result<()> {
            self.peer
                .sender
                .lock()
                .await
                .send_message_with::<HelloMessageCodec>(&HelloMessage {
                    version: make_bitflags!(NodeFinderVersion::V1),
                })
                .await
        }

        async fn wait_registered(&self) -> Result<()> {
            tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
                while self.sessions.len().await == 0 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?;
            Ok(())
        }

        async fn wait_closed(&self, timeout: Duration) -> Result<usize> {
            let count = tokio::time::timeout(timeout, async {
                let mut count = 0;
                loop {
                    match self.peer.receiver.lock().await.recv().await {
                        Ok(frame) => {
                            assert_eq!(DataMessageCodec::decode(&frame)?, DataMessage::default());
                            count += 1;
                        }
                        Err(e) => {
                            assert_eq!(e.kind(), &omnius_core_omnikit::ErrorKind::EndOfStream);
                            break;
                        }
                    }
                }
                Result::Ok(count)
            })
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))??;
            assert_eq!(self.sessions.len().await, 0);
            Ok(count)
        }
    }

    #[tokio::test]
    async fn handshake_succeeds_with_common_version() -> TestResult {
        let (session, peer, peer_node_profile) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, peer_node_profile.clone()));

        let (received, _) = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout()).await?;
        assert_eq!(received, peer_node_profile);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_peer_without_common_version() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(async move {
            peer.sender
                .lock()
                .await
                .send_message_with::<HelloMessageCodec>(&HelloMessage { version: BitFlags::empty() })
                .await?;
            let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
            Result::Ok(())
        });

        let err = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::UnsupportedType);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_peer_with_own_id() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, node_profile("me")));

        let err = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::Reject);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_profile_that_does_not_match_the_certificate() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, node_profile("someone else")));

        let err = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::Reject);
        peer_task.await??;

        Ok(())
    }

    async fn create_task_communicator(state_dir: &str, sessions: Arc<SessionRegistry>) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        create_task_communicator_with_intervals(state_dir, sessions, NodeFinderIntervals::default()).await
    }

    async fn create_task_communicator_with_intervals(
        state_dir: &str,
        sessions: Arc<SessionRegistry>,
        intervals: NodeFinderIntervals,
    ) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        create_task_communicator_with_profile(state_dir, sessions, node_profile("me"), intervals).await
    }

    async fn create_task_communicator_with_profile(
        state_dir: &str,
        sessions: Arc<SessionRegistry>,
        my_node_profile: NodeProfile,
        intervals: NodeFinderIntervals,
    ) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        create_task_communicator_with_sleeper(state_dir, sessions, my_node_profile, intervals, Arc::new(SleeperImpl)).await
    }

    async fn create_task_communicator_with_sleeper(
        state_dir: &str,
        sessions: Arc<SessionRegistry>,
        my_node_profile: NodeProfile,
        intervals: NodeFinderIntervals,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let (session_sender, session_receiver) = mpsc::channel(20);

        let task = TaskCommunicator::new(
            Arc::new(Mutex::new(my_node_profile)),
            sessions,
            Arc::new(NodeFinderRepo::new(state_dir, clock).await?),
            Arc::new(TokioMutex::new(session_receiver)),
            sleeper,
            NodeFinderOption {
                state_dir: state_dir.to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
                intervals,
            },
        )
        .await?;

        Ok((task, session_sender))
    }

    /// 要求された待ち時間を記録して、実際に待つ Sleeper。
    struct RecordingSleeper {
        requested: Mutex<Vec<Duration>>,
    }

    #[async_trait]
    impl Sleeper for RecordingSleeper {
        async fn sleep(&self, duration: Duration) {
            self.requested.lock().push(duration);
            tokio::time::sleep(duration).await;
        }
    }

    async fn answer_handshake(peer: FramedStream, node_profile: NodeProfile) -> Result<()> {
        peer.sender
            .lock()
            .await
            .send_message_with::<HelloMessageCodec>(&HelloMessage {
                version: make_bitflags!(NodeFinderVersion::V1),
            })
            .await?;
        let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        let message = crate::protocol::tests::legacy::node::ProfileMessage {
            node_profile: crate::protocol::tests::legacy_profile(&node_profile),
        };
        peer.sender.lock().await.send(message.export()?.into()).await?;
        let _: ProfileMessage = peer.receiver.lock().await.recv_message_with::<ProfileMessageCodec>().await?;
        Ok(())
    }

    /// local 側の Session と、相手側の stream および Session の cert と一致する相手の NodeProfile を返す
    fn session_pair() -> Result<(Session, FramedStream, NodeProfile)> {
        let (local, peer) = tokio::io::duplex(64 * 1024);
        let (local_reader, local_writer) = tokio::io::split(local);
        let (peer_reader, peer_writer) = tokio::io::split(peer);

        let peer_signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?;
        let peer_cert = peer_signer.sign(b"test")?;
        let peer_node_profile = NodeProfile::new(peer_cert.public_key.clone(), vec![]);
        let session = Session {
            typ: SessionType::NodeFinder,
            address: OmniAddr::create_tcp("127.0.0.1".parse()?, 1),
            handshake_type: SessionHandshakeType::Connected,
            cert: peer_cert,
            stream: FramedStream::new(local_reader, local_writer),
        };

        Ok((session, FramedStream::new(peer_reader, peer_writer), peer_node_profile))
    }

    fn node_profile(public_key: &str) -> NodeProfile {
        NodeProfile::new(public_key.as_bytes().to_vec(), vec![])
    }
}
