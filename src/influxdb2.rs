use eyre::Result;
use futures::prelude::*;
use influxdb2::api::write::TimestampPrecision;
use influxdb2::models::DataPoint;
use influxdb2::Client;

use crate::api::SpotPrice;
use crate::config::InfluxDB2Config;

pub async fn publish_to_influxdb2(prices: &[SpotPrice], config: &InfluxDB2Config) -> Result<()> {
    let client = Client::new(
        config.host.clone(),
        config.org.clone(),
        config.token.clone(),
    );

    let points: Result<Vec<DataPoint>, _> = prices
        .iter()
        .map(|p| {
            DataPoint::builder("price")
                .tag("id", "price")
                .field("price", p.price)
                .timestamp(p.start_date.timestamp_millis())
                .build()
        })
        .collect();

    let result = client
        .write_with_precision(
            &config.bucket,
            stream::iter(points?),
            TimestampPrecision::Milliseconds,
        )
        .await;

    if let Err(e) = result {
        error!("Error writing to InfluxDB: {}", e);
    } else {
        info!("Successfully wrote to InfluxDB");
    }

    Ok(())
}
