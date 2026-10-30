use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::MqttConfig;
use rand::{distr::Alphanumeric, RngExt};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct Mqtt {
    client: AsyncClient,
    topic: String,
    /// Most recently published payload, republished after every (re)connect
    last_payload: Arc<Mutex<Option<String>>>,
}

impl Mqtt {
    /// Publishes a retained message to the configured topic.
    ///
    /// Never blocks: if the broker is unreachable, the message is
    /// republished once the connection comes back.
    pub fn publish(&self, payload: String) {
        *self.last_payload.lock().unwrap() = Some(payload.clone());
        try_publish(&self.client, &self.topic, payload);
    }
}

fn try_publish(client: &AsyncClient, topic: &str, payload: String) {
    if let Err(e) = client.try_publish(topic, QoS::AtLeastOnce, true, payload) {
        warn!("Could not queue MQTT publish, will retry after reconnect: {e}");
    }
}

/// Builds a unique MQTT client id by appending a random suffix to `id`.
fn client_id(id: &str) -> String {
    let random_string: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    format!("{id}-{random_string}")
}

pub fn init_mqtt(config: &MqttConfig) -> (Mqtt, JoinHandle<()>) {
    let mut options = MqttOptions::new(client_id(&config.id), config.host.clone(), config.port);
    options.set_keep_alive(Duration::from_secs(15));
    if let Some(username) = &config.username {
        options.set_credentials(
            username.clone(),
            config.password.clone().unwrap_or_default(),
        );
    }
    let (client, mut eventloop) = AsyncClient::new(options, 10);

    let mqtt = Mqtt {
        client: client.clone(),
        topic: config.topic.clone(),
        last_payload: Default::default(),
    };

    let last_payload = mqtt.last_payload.clone();
    let topic = config.topic.clone();

    let handle = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    info!("Connected to MQTT broker");

                    // rumqttc drops anything queued before a disconnect when
                    // starting a new session, so republish our latest state.
                    // Must not await here: the event loop is what drains the
                    // request channel, awaiting a full channel would deadlock.
                    let payload = last_payload.lock().unwrap().clone();
                    if let Some(payload) = payload {
                        try_publish(&client, &topic, payload);
                    }
                }
                Ok(e) => trace!("MQTT event: {e:?}"),
                Err(e) => {
                    error!("MQTT error: {e}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });

    (mqtt, handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_has_random_suffix() {
        let a = client_id("nordpool-mqtt");
        let b = client_id("nordpool-mqtt");

        assert!(a.starts_with("nordpool-mqtt-"));
        assert_eq!(a.len(), "nordpool-mqtt-".len() + 8);
        assert!(a["nordpool-mqtt-".len()..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }
}
