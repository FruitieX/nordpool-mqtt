use std::time::Duration;

use chrono::Utc;
use tokio::sync::watch;

use crate::{api::SpotPrice, mqtt::Mqtt, prices::PriceStore, time::sleep_until};

/// Publishes the current price to MQTT whenever it changes
pub async fn run_publisher(mut rx: watch::Receiver<PriceStore>, mqtt: Mqtt) -> eyre::Result<()> {
    let mut last_published: Option<SpotPrice> = None;
    let mut warned_missing = false;

    loop {
        let now = Utc::now();
        let current = rx.borrow_and_update().current(now).cloned();

        let wake_at = match current {
            Some(price) => {
                warned_missing = false;

                if last_published.as_ref() != Some(&price) {
                    let json = serde_json::json!({
                        "id": "price",
                        "name": "price",
                        "price": price.price,
                        "start_date": price.start_date.to_rfc3339(),
                        "end_date": price.end_date.to_rfc3339(),
                        "sensor_value": price.price,
                    });

                    info!("Publishing current price: {json}");
                    mqtt.publish(json.to_string());
                }

                let end_date = price.end_date;
                last_published = Some(price);
                end_date
            }
            None => {
                if !warned_missing {
                    warn!("No price available for {now}, waiting for prices to be fetched");
                    warned_missing = true;
                }

                now + Duration::from_secs(60)
            }
        };

        tokio::select! {
            _ = sleep_until(wake_at) => {}
            res = rx.changed() => res?,
        }
    }
}
