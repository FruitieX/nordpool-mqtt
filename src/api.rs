use chrono::{DateTime, Utc};
use eyre::Result;
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotPrice {
    pub price: f64,
    pub start_date: DateTime<Utc>,
}

#[derive(Deserialize, Default)]
struct Response {
    prices: Vec<SpotPrice>,
}

pub async fn fetch_prices() -> Result<Vec<SpotPrice>> {
    let resp: Response = Client::new()
        .get("https://api.porssisahko.net/v1/latest-prices.json")
        .send()
        .await?
        .json()
        .await?;

    Ok(resp.prices)
}
