use crate::config::Config;
use eyre::Result;
use rand::{distributions::Alphanumeric, Rng};
use rumqttc::{AsyncClient, MqttOptions};
use std::time::Duration;

pub fn init_mqtt(config: &Config) -> Result<AsyncClient> {
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    let mut options = MqttOptions::new(
        format!("{}-{}", config.mqtt.id, random_string),
        config.mqtt.host.clone(),
        config.mqtt.port,
    );
    options.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(options, 10);

    tokio::spawn(async move {
        loop {
            let res = eventloop.poll().await;

            match res {
                Ok(e) => trace!("MQTT event: {e:?}"),
                Err(e) => {
                    error!("MQTT error: {e}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });

    Ok(client)
}
