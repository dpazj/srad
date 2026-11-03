//! Minimal MQTT client used to drive the TCK.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;

pub const RESULT_TOPIC: &str = "SPARKPLUG_TCK/RESULT";
pub const LOG_TOPIC: &str = "SPARKPLUG_TCK/LOG";
pub const CONFIG_TOPIC: &str = "SPARKPLUG_TCK/CONFIG";
pub const TEST_CONTROL_TOPIC: &str = "SPARKPLUG_TCK/TEST_CONTROL";
pub const CONSOLE_PROMPT_TOPIC: &str = "SPARKPLUG_TCK/CONSOLE_PROMPT";
pub const CONSOLE_REPLY_TOPIC: &str = "SPARKPLUG_TCK/CONSOLE_REPLY";

#[derive(Debug)]
pub enum Message {
    Connected,
    Publish { topic: String, payload: String },
    Error(String),
}

pub struct Driver {
    client: AsyncClient,
    events: mpsc::UnboundedReceiver<Message>,
}

impl Driver {
    /// Connect to the broker and subscribe to the TCK control topics.
    pub async fn connect(host: &str, port: u16, connect_timeout: Duration) -> Result<Self> {
        let client_id = format!("srad-tck-runner-{}", std::process::id());
        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_clean_session(true);

        let (client, mut eventloop) = AsyncClient::new(options, 50);

        for topic in [RESULT_TOPIC, LOG_TOPIC, CONSOLE_PROMPT_TOPIC] {
            client
                .subscribe(topic, QoS::AtLeastOnce)
                .await
                .with_context(|| format!("subscribing to {topic}"))?;
        }

        let (tx, events) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        if tx.send(Message::Connected).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let message = Message::Publish {
                            topic: publish.topic,
                            payload: String::from_utf8_lossy(&publish.payload).into_owned(),
                        };
                        if tx.send(message).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = tx.send(Message::Error(error.to_string()));
                        break;
                    }
                }
            }
        });

        let mut driver = Self { client, events };
        driver.wait_connected(connect_timeout).await?;
        Ok(driver)
    }

    async fn wait_connected(&mut self, connect_timeout: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + connect_timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                bail!("timed out waiting for the MQTT connection");
            }
            match tokio::time::timeout(remaining, self.events.recv()).await {
                Ok(Some(Message::Connected)) => return Ok(()),
                Ok(Some(Message::Error(error))) => bail!("MQTT connection failed: {error}"),
                Ok(Some(_)) => continue,
                Ok(None) => bail!("MQTT event loop stopped"),
                Err(_) => bail!("timed out waiting for the MQTT connection"),
            }
        }
    }

    pub async fn publish(&self, topic: &str, payload: &str) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .with_context(|| format!("publishing to {topic}"))
    }

    pub async fn publish_retained(&self, topic: &str, payload: &str) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, true, payload)
            .await
            .with_context(|| format!("publishing retained message to {topic}"))
    }

    pub async fn next(&mut self) -> Option<Message> {
        self.events.recv().await
    }
}
