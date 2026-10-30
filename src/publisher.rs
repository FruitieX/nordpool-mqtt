use std::time::Duration;

use chrono::Utc;
use tokio::sync::watch;

use crate::{api::SpotPrice, mqtt::Mqtt, prices::PriceStore, time::sleep_until};

/// MQTT payload for the current price
fn price_payload(price: &SpotPrice) -> serde_json::Value {
    serde_json::json!({
        "id": "price",
        "name": "price",
        "price": price.price,
        "start_date": price.start_date.to_rfc3339(),
        "end_date": price.end_date.to_rfc3339(),
        "sensor_value": price.price,
    })
}

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
                    let json = price_payload(&price);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_price_payload() {
        let price = SpotPrice {
            price: 3.554,
            start_date: "2026-10-14T05:00:00Z".parse().unwrap(),
            end_date: "2026-10-14T06:00:00Z".parse().unwrap(),
        };

        let payload: serde_json::Value =
            serde_json::from_str(&price_payload(&price).to_string()).unwrap();

        assert_eq!(
            payload,
            serde_json::json!({
                "id": "price",
                "name": "price",
                "price": 3.554,
                "start_date": "2026-10-14T05:00:00+00:00",
                "end_date": "2026-10-14T06:00:00+00:00",
                "sensor_value": 3.554,
            })
        );
    }
}
