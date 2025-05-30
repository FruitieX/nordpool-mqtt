use chrono::{DateTime, Duration, DurationRound, Utc};
use croner::Cron;
use eyre::{eyre, Result};

use crate::{
    api::{fetch_prices, SpotPrice},
    config::Config,
    influxdb2::publish_to_influxdb2,
};

pub struct State {
    pub config: Config,
    pub cron: Cron,
    /// start_date of previously published price
    pub prev_price_t: DateTime<Utc>,
    pub mqtt_client: rumqttc::AsyncClient,
    pub prices: Vec<SpotPrice>,
    pub initial_run: bool,
}

impl State {
    pub fn new(config: Config, cron: Cron, mqtt_client: rumqttc::AsyncClient) -> Self {
        Self {
            config,
            cron,
            prev_price_t: Utc::now(),
            mqtt_client,
            prices: Default::default(),
            initial_run: true,
        }
    }

    pub async fn process(&mut self) -> Result<()> {
        let t = if self.initial_run {
            self.initial_run = false;

            // At initial run, we should publish the current hour's price immediately
            Utc::now().duration_trunc(Duration::hours(1))?
        } else {
            // At subsequent runs, we should wait until the next hour and then publish the price
            let next = self
                .cron
                .find_next_occurrence(&self.prev_price_t, false)
                .expect("Could not find next occurrence that matches the cron pattern");

            let wait_duration = next - Utc::now();

            if wait_duration.num_seconds() > 0 {
                debug!("Waiting until next hour: {next}");
                tokio::time::sleep(wait_duration.to_std()?).await;
            }

            next
        };

        self.prev_price_t = t;
        let remaining_prices = self.prices.iter().filter(|p| t <= p.start_date).count();

        // Nordpool releases their prices around 13:00 CET, if there are 10 or
        // fewer prices left in the list, we should check if new prices have
        // been published
        let fetched_new_prices = if remaining_prices <= 10 {
            info!("Fetching prices");
            self.prices = fetch_prices().await?;
            self.prices.iter().filter(|p| t <= p.start_date).count() > remaining_prices
        } else {
            false
        };

        // Publish new prices to InfluxDB
        if fetched_new_prices {
            if let Some(config) = &self.config.influxdb2 {
                let config = config.clone();
                let prices = self.prices.clone();
                tokio::spawn(async move {
                    let _ = publish_to_influxdb2(&prices, &config).await;
                });
            }
        }

        let price = self
            .prices
            .iter()
            .find(|p| t == p.start_date)
            .ok_or_else(|| eyre!("Could not find price for current hour"))?;

        let json = serde_json::json!({
            "id": "price",
            "name": "price",
            "price": price.price,
            "start_date": price.start_date.to_rfc3339(),
            "sensor_value": price.price,
        });

        info!("Publishing current price: {}", json);

        self.mqtt_client
            .publish(
                self.config.mqtt.topic.clone(),
                rumqttc::QoS::AtLeastOnce,
                true,
                json.to_string(),
            )
            .await?;

        Ok(())
    }
}
