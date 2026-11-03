use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use log::{debug, error, info, warn};
use srad_client::{
    Client, ClientError, DeviceMessage, DynClient, DynEventLoop, Event, EventLoop, NodeMessage,
    StatePayload,
};
use srad_types::{
    constants::NODE_CONTROL_REBIRTH,
    payload::Payload,
    topic::{DeviceTopic, NodeTopic, QoS, StateTopic, Topic, TopicFilter},
    utils::{self, timestamp},
    MetricId,
};

use crate::{
    config::SubscriptionConfig,
    events::{AppDeviceEvent, AppNodeEvent, PayloadErrorDetails},
    metrics::PublishMetric,
};

use thiserror::Error;

use tokio::{
    select,
    sync::mpsc::{self, Receiver},
    task,
    time::{sleep_until, timeout, Instant as TokioInstant},
};

struct Shutdown;

#[derive(Debug, Clone)]
enum PublishTopicKind {
    NodeTopic(NodeTopic),
    DeviceTopic(DeviceTopic),
}

/// Configuration for a topic to publish a Metric on
#[derive(Debug, Clone)]
pub struct PublishTopic(PublishTopicKind);

impl PublishTopic {
    /// Create a new `PublishTopic` which will publish on a specified device's CMD topic
    pub fn new_device_cmd(group_id: &str, node_id: &str, device_id: &str) -> Self {
        PublishTopic(PublishTopicKind::DeviceTopic(DeviceTopic::new(
            group_id,
            srad_types::topic::DeviceMessage::DCmd,
            node_id,
            device_id,
        )))
    }

    /// Create a new `PublishTopic` which will publish on a specified node's CMD topic
    pub fn new_node_cmd(group_id: &str, node_id: &str) -> Self {
        PublishTopic(PublishTopicKind::NodeTopic(NodeTopic::new(
            group_id,
            srad_types::topic::NodeMessage::NCmd,
            node_id,
        )))
    }
}

/// The client to interact with the [AppEventLoop] and Sparkplug namespace from.
#[derive(Clone)]
pub struct AppClient {
    client: Arc<DynClient>,
    sender: mpsc::Sender<Shutdown>,
    state: Arc<AppState>,
}

impl AppClient {
    /// Stop all operations, sending a death certificate (application offline message) and disconnect from the broker.
    ///
    /// This will produce an [AppEvent::Cancelled] event on the [AppEventLoop] after the application has gracefully disconnected.
    ///
    /// The death certificate and the MQTT disconnect are queued on the client,
    /// so the caller must keep polling the [AppEventLoop] until it returns
    /// [AppEvent::Cancelled] (or the client is disconnected) to ensure they are
    /// flushed before the process exits.
    pub async fn cancel(&self) {
        info!("App Stopping");
        let topic = StateTopic::new_host(&self.state.host_id);
        match self
            .client
            .try_publish_state_message(
                topic,
                srad_client::StatePayload::Offline {
                    timestamp: timestamp(),
                },
            )
            .await
        {
            Ok(_) => (),
            Err(_) => debug!("Unable to publish state offline on exit"),
        };
        _ = self.sender.send(Shutdown).await;
        _ = self.client.disconnect().await;
    }

    /// Issue a node rebirth CMD request
    pub async fn publish_node_rebirth(
        &self,
        group_id: &str,
        node_id: &str,
    ) -> Result<(), ClientError> {
        let topic = PublishTopic::new_node_cmd(group_id, node_id);
        let rebirth_cmd = PublishMetric::new(MetricId::Name(NODE_CONTROL_REBIRTH.into()), true);
        self.publish_metrics(topic, vec![rebirth_cmd]).await
    }

    fn metrics_to_payload(metrics: Vec<PublishMetric>) -> Payload {
        let mut payload_metrics = Vec::with_capacity(metrics.len());
        for x in metrics.into_iter() {
            payload_metrics.push(x.into());
        }
        Payload {
            timestamp: Some(utils::timestamp()),
            metrics: payload_metrics,
            seq: None,
            uuid: None,
            body: None,
        }
    }

    /// Attempts to publish metrics on a specified topic. Uses the `try_publish` methods from the [srad_client::Client] trait.
    pub async fn try_publish_metrics(
        &self,
        topic: PublishTopic,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), ClientError> {
        let payload = Self::metrics_to_payload(metrics);
        match topic.0 {
            PublishTopicKind::NodeTopic(topic) => {
                self.client.try_publish_node_message(topic, payload).await
            }
            PublishTopicKind::DeviceTopic(topic) => {
                self.client.try_publish_device_message(topic, payload).await
            }
        }
    }

    /// Publish metrics on a specified topic. Uses the `publish` methods from the [srad_client::Client] trait.
    pub async fn publish_metrics(
        &self,
        topic: PublishTopic,
        metrics: Vec<PublishMetric>,
    ) -> Result<(), ClientError> {
        let payload = Self::metrics_to_payload(metrics);
        match topic.0 {
            PublishTopicKind::NodeTopic(topic) => {
                self.client.publish_node_message(topic, payload).await
            }
            PublishTopicKind::DeviceTopic(topic) => {
                self.client.publish_device_message(topic, payload).await
            }
        }
    }
}

struct AppState {
    host_id: String,
    published_online_state: AtomicBool,
}

/// Errors that prevent an application from establishing its session.
#[derive(Debug, Error)]
enum SessionError {
    /// The client failed to subscribe to the topics required by the application.
    #[error("failed to subscribe to required topics: {0}")]
    Subscribe(#[source] ClientError),
    /// The application could not publish its online STATE message.
    #[error("failed to publish the application online state: {0}")]
    PublishOnlineState(#[source] ClientError),
}

/// A Sparkplug Application EventLoop
///
/// On top of [srad_client::EventLoop] functionality, AppEventLoop provides some specific application functionality:
///
/// * Topic Subscription setup and management
/// * Application State message publishing
/// * Topic specific message payload verification and transformation
pub struct AppEventLoop {
    online: bool,
    /// Whether the underlying client currently reports an established connection.
    client_online: bool,
    /// Whether a session establishment task is currently running.
    session_in_flight: bool,
    /// Identifies the current session establishment attempt. Results from older
    /// attempts are ignored.
    session_attempt: u64,
    /// When the next attempt to establish the application session may be made.
    retry_at: Option<Instant>,
    retry_delay: Duration,
    state: Arc<AppState>,
    will_timestamp: u64,
    subscription_config: SubscriptionConfig,
    client: AppClient,
    eventloop: Box<DynEventLoop>,
    shutdown_rx: Receiver<Shutdown>,
    session_tx: mpsc::Sender<(u64, Result<(), SessionError>)>,
    session_rx: mpsc::Receiver<(u64, Result<(), SessionError>)>,
}

impl AppEventLoop {
    /// Creates a new instance along with an associated client.
    pub fn new<
        S: Into<String>,
        E: EventLoop + Send + 'static,
        C: Client + Send + Sync + 'static,
    >(
        host_id: S,
        subscription_config: SubscriptionConfig,
        eventloop: E,
        client: C,
    ) -> (Self, AppClient) {
        let (tx, rx) = mpsc::channel(1);

        let host_id: String = host_id.into();
        if let Err(e) = utils::validate_name(&host_id) {
            panic!("Invalid host id: {e}");
        };

        let app_state = Arc::new(AppState {
            host_id,
            published_online_state: AtomicBool::new(false),
        });
        let client = AppClient {
            client: Arc::new(client),
            sender: tx,
            state: app_state.clone(),
        };
        let (session_tx, session_rx) = mpsc::channel(1);
        let mut app = Self {
            online: false,
            client_online: false,
            session_in_flight: false,
            session_attempt: 0,
            retry_at: None,
            retry_delay: Duration::from_secs(3),
            state: app_state,
            will_timestamp: 0,
            client: client.clone(),
            eventloop: Box::new(eventloop),
            subscription_config,
            shutdown_rx: rx,
            session_tx,
            session_rx,
        };
        app.update_last_will();
        (app, client)
    }

    /// Sets the delay between attempts to establish the application session.
    ///
    /// If the application cannot subscribe to its topics or publish its online
    /// STATE it disconnects and retries after this delay. Defaults to 3 seconds.
    pub fn set_retry_delay(&mut self, delay: Duration) {
        self.retry_delay = delay;
    }

    fn update_last_will(&mut self) {
        self.will_timestamp = timestamp();
        self.eventloop.set_last_will(srad_client::LastWill::new_app(
            &self.state.host_id,
            self.will_timestamp,
        ));
    }

    /// The topic the application publishes its STATE messages on and the
    /// subscriptions it needs for the session.
    fn session_topics(&self) -> (StateTopic, Vec<TopicFilter>) {
        let state_topic = StateTopic::new_host(&self.state.host_id);
        let mut topics: Vec<TopicFilter> = self.subscription_config.clone().into();
        // Subscribe to the application's own STATE topic even when the
        // namespace subscription already covers it. The Sparkplug TCK requires
        // the explicit STATE subscription.
        topics.push(TopicFilter::new_with_qos(
            Topic::State(state_topic.clone()),
            QoS::AtMostOnce,
        ));
        (state_topic, topics)
    }

    /// Attempt to establish the application session, retrying on a delay if it
    /// fails.
    ///
    /// The subscribe and online STATE publish run on a spawned task so that the
    /// event loop keeps being polled. Awaiting the client from here would block
    /// the event loop, and with a zero-capacity request channel the client would
    /// wait forever for the event loop to receive the request.
    fn try_online(&mut self) {
        if !self.client_online || self.online || self.retry_at.is_some() || self.session_in_flight {
            return;
        }

        info!("App Online");
        let (state_topic, topics) = self.session_topics();
        let client = self.client.client.clone();
        let timestamp = self.will_timestamp;
        let tx = self.session_tx.clone();

        self.session_attempt = self.session_attempt.wrapping_add(1);
        let attempt = self.session_attempt;
        self.session_in_flight = true;

        task::spawn(async move {
            let result = async {
                client
                    .subscribe_many(topics)
                    .await
                    .map_err(SessionError::Subscribe)?;

                // Only publish the online state once the subscriptions have been
                // established, otherwise the application may miss messages while
                // advertising itself online.
                client
                    .publish_state_message(state_topic, StatePayload::Online { timestamp })
                    .await
                    .map_err(SessionError::PublishOnlineState)
            }
            .await;

            _ = tx.send((attempt, result)).await;
        });
    }

    fn on_session_established(&mut self) -> Option<AppEvent> {
        self.state
            .published_online_state
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.online = true;
        Some(AppEvent::Online)
    }

    fn on_session_failed(&mut self, error: SessionError) {
        warn!("Failed to establish application session: {error}");
        self.state
            .published_online_state
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.retry_at = Some(Instant::now() + self.retry_delay);

        // Disconnect on a task too: the request channel may have zero capacity
        // and the event loop must keep being polled.
        let client = self.client.client.clone();
        task::spawn(async move {
            _ = client.disconnect().await;
        });
    }

    fn handle_offline(&mut self) -> Option<AppEvent> {
        if !self.online {
            return None;
        }
        info!("App Offline");
        self.online = false;
        self.state
            .published_online_state
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.update_last_will();
        Some(AppEvent::Offline)
    }

    fn handle_node_message(message: NodeMessage) -> Result<Option<AppEvent>, PayloadErrorDetails> {
        match AppNodeEvent::try_from(message) {
            Ok(v) => Ok(Some(AppEvent::Node(v))),
            Err(e) => match e {
                crate::events::MessageTryFromError::PayloadError(e) => Err(e),
                crate::events::MessageTryFromError::UnsupportedVerb => Ok(None),
            },
        }
    }

    fn handle_device_message(
        message: DeviceMessage,
    ) -> Result<Option<AppEvent>, PayloadErrorDetails> {
        match AppDeviceEvent::try_from(message) {
            Ok(v) => Ok(Some(AppEvent::Device(v))),
            Err(e) => match e {
                crate::events::MessageTryFromError::PayloadError(e) => Err(e),
                crate::events::MessageTryFromError::UnsupportedVerb => Ok(None),
            },
        }
    }

    fn handle_event(&mut self, event: Event) -> Option<AppEvent> {
        match event {
            Event::Offline => {
                self.client_online = false;
                // Invalidate any in-flight session establishment for this connection.
                self.session_attempt = self.session_attempt.wrapping_add(1);
                self.session_in_flight = false;
                self.handle_offline()
            }
            Event::Online => {
                self.client_online = true;
                self.try_online();
                None
            }
            Event::Node(message) => match Self::handle_node_message(message) {
                Ok(event) => event,
                Err(e) => Some(AppEvent::InvalidPayload(e)),
            },
            Event::Device(message) => match Self::handle_device_message(message) {
                Ok(event) => event,
                Err(e) => Some(AppEvent::InvalidPayload(e)),
            },
            Event::State { host_id, payload } => {
                if self
                    .client
                    .state
                    .published_online_state
                    .load(std::sync::atomic::Ordering::SeqCst)
                    && host_id == self.client.state.host_id
                {
                    if let StatePayload::Offline { timestamp: _ } = payload {
                        let topic = StateTopic::new_host(&self.client.state.host_id);
                        let client = self.client.client.clone();
                        let timestamp = self.will_timestamp;
                        task::spawn(async move {
                            if let Err(e) = client
                                .publish_state_message(topic, StatePayload::Online { timestamp })
                                .await
                            {
                                error!("Failed to republish application online state: {e}");
                            }
                        });
                    }
                }
                None
            }
            _ => None,
        }
    }

    async fn poll_until_offline(&mut self) {
        while self.online {
            if Event::Offline == self.eventloop.poll().await {
                self.handle_offline();
            }
        }
    }

    async fn poll_until_offline_with_timeout(&mut self) {
        _ = timeout(Duration::from_secs(1), self.poll_until_offline()).await;
    }

    /// Progress the App. Continuing to poll will reconnect if the application if there is a disconnection.
    /// **NOTE** Don't block this while iterating. Additionally, using the `AppClient` directly inside the loop may block progress.
    ///
    /// If the application cannot establish its session, for example because
    /// subscribing to its topics or publishing its online STATE fails, it
    /// disconnects and retries after a delay rather than stopping.
    pub async fn poll(&mut self) -> AppEvent {
        loop {
            let retry_at = TokioInstant::from_std(self.retry_at.unwrap_or_else(Instant::now));
            select! {
                event = self.eventloop.poll() => {
                    if let Some(app_event) = self.handle_event(event) {
                        return app_event
                    }
                }
                Some((attempt, result)) = self.session_rx.recv() => {
                    if attempt != self.session_attempt {
                        // Result from a connection that is no longer current.
                        continue;
                    }
                    self.session_in_flight = false;
                    if !self.client_online {
                        continue;
                    }
                    match result {
                        Ok(()) => {
                            if let Some(app_event) = self.on_session_established() {
                                return app_event
                            }
                        }
                        Err(e) => self.on_session_failed(e),
                    }
                }
                Some(_) = self.shutdown_rx.recv() => {
                    self.poll_until_offline_with_timeout().await;
                    return AppEvent::Cancelled
                },
                _ = sleep_until(retry_at), if self.retry_at.is_some() => {
                    self.retry_at = None;
                    self.try_online();
                },
            }
        }
    }
}

/// An event produced by the [AppEventLoop]
#[derive(Debug)]
pub enum AppEvent {
    /// Application is online and connected to the broker
    Online,
    /// Application is offline and has disconnected from the broker
    Offline,
    Device(AppDeviceEvent),
    Node(AppNodeEvent),
    /// Application has received an invalid payload where it may wish to issue a Rebirth request
    InvalidPayload(PayloadErrorDetails),
    Cancelled,
}
