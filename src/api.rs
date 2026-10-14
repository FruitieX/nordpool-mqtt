use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use eyre::Result;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::config::PricesConfig;

const API_URL: &str = "https://dataportal-api.nordpoolgroup.com/api/DayAheadPriceIndices";

#[derive(Debug, Clone, PartialEq)]
pub struct SpotPrice {
    /// Price in cents (or 1/100 of `currency`) per kWh, VAT included
    pub price: f64,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
}

/// Prices for one delivery day (CET)
#[derive(Debug, Clone, PartialEq)]
pub struct DayPrices {
    pub prices: Vec<SpotPrice>,
    /// Nord Pool may publish preliminary prices that get corrected later
    pub is_final: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    #[serde(default)]
    area_states: Vec<AreaState>,
    #[serde(default)]
    multi_index_entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct AreaState {
    state: String,
    areas: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    delivery_start: DateTime<Utc>,
    delivery_end: DateTime<Utc>,
    entry_per_area: HashMap<String, Option<f64>>,
}

pub struct NordpoolClient {
    client: Client,
    config: PricesConfig,
}

impl NordpoolClient {
    pub fn new(config: PricesConfig) -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("nordpool-mqtt/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { client, config })
    }

    /// Fetches prices for the given CET delivery day. Returns `None` if
    /// prices for that day have not been published yet.
    pub async fn fetch_day(&self, date: NaiveDate) -> Result<Option<DayPrices>> {
        let resp = self
            .client
            .get(API_URL)
            .query(&[
                ("date", date.to_string()),
                ("market", "DayAhead".to_string()),
                ("indexNames", self.config.area.clone()),
                ("currency", self.config.currency.clone()),
                (
                    "resolutionInMinutes",
                    self.config.resolution_minutes.to_string(),
                ),
            ])
            .send()
            .await?
            .error_for_status()?;

        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let body: Response = resp.json().await?;

        Ok(parse_response(body, &self.config))
    }
}

fn parse_response(body: Response, config: &PricesConfig) -> Option<DayPrices> {
    let area = &config.area;
    let vat_multiplier = 1.0 + config.vat_percent / 100.0;

    let prices: Vec<SpotPrice> = body
        .multi_index_entries
        .into_iter()
        .filter_map(|entry| {
            let price_per_mwh = (*entry.entry_per_area.get(area)?)?;
            // Currency per MWh -> cents per kWh, with VAT
            let price = price_per_mwh / 10.0 * vat_multiplier;

            Some(SpotPrice {
                price: (price * 1000.0).round() / 1000.0,
                start_date: entry.delivery_start,
                end_date: entry.delivery_end,
            })
        })
        .collect();

    if prices.is_empty() {
        return None;
    }

    let is_final = body
        .area_states
        .iter()
        .any(|s| s.state == "Final" && s.areas.contains(area));

    Some(DayPrices { prices, is_final })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_published_day() {
        let body: Response = serde_json::from_str(
            r#"{
                "deliveryDateCET": "2026-10-14",
                "areaStates": [{ "state": "Final", "areas": ["FI"] }],
                "multiIndexEntries": [
                    {
                        "deliveryStart": "2026-10-13T22:00:00Z",
                        "deliveryEnd": "2026-10-13T23:00:00Z",
                        "entryPerArea": { "FI": 6.16 }
                    },
                    {
                        "deliveryStart": "2026-10-14T05:00:00Z",
                        "deliveryEnd": "2026-10-14T06:00:00Z",
                        "entryPerArea": { "FI": 28.32 }
                    }
                ]
            }"#,
        )
        .unwrap();

        let day = parse_response(body, &PricesConfig::default()).unwrap();

        assert!(day.is_final);
        assert_eq!(day.prices.len(), 2);
        // Matches what api.porssisahko.net reported for the same hours
        assert_eq!(day.prices[0].price, 0.773);
        assert_eq!(day.prices[1].price, 3.554);
        assert_eq!(
            day.prices[0].start_date.to_rfc3339(),
            "2026-10-13T22:00:00+00:00"
        );
    }

    #[test]
    fn unpublished_day_is_none() {
        let body: Response = serde_json::from_str(
            r#"{
                "deliveryDateCET": "2026-10-15",
                "version": 0,
                "areaStates": [],
                "multiIndexEntries": []
            }"#,
        )
        .unwrap();

        assert_eq!(parse_response(body, &PricesConfig::default()), None);
    }

    #[test]
    fn preliminary_day_is_not_final() {
        let body: Response = serde_json::from_str(
            r#"{
                "areaStates": [{ "state": "Preliminary", "areas": ["FI"] }],
                "multiIndexEntries": [
                    {
                        "deliveryStart": "2026-10-13T22:00:00Z",
                        "deliveryEnd": "2026-10-13T23:00:00Z",
                        "entryPerArea": { "FI": 6.16, "SE3": null }
                    }
                ]
            }"#,
        )
        .unwrap();

        let day = parse_response(body, &PricesConfig::default()).unwrap();
        assert!(!day.is_final);
    }
}
