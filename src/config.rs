use chrono::NaiveTime;
use eyre::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct MqttConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub topic: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct InfluxDB2Config {
    pub host: String,
    pub org: String,
    pub token: String,
    pub bucket: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PricesConfig {
    /// Nord Pool delivery area, e.g. "FI", "SE3", "EE"
    pub area: String,

    /// Currency to fetch prices in, e.g. "EUR", "SEK"
    pub currency: String,

    /// Price resolution in minutes, 15 or 60. With 60, the hourly average
    /// published by Nord Pool is used.
    pub resolution_minutes: u32,

    /// VAT percentage added to prices
    pub vat_percent: f64,

    /// Time of day (Central European Time) at which we start polling for the
    /// next day's prices. Nord Pool usually publishes them around 12:45 CET.
    pub poll_start: NaiveTime,

    /// How often to poll while waiting for prices to be published
    pub poll_interval_secs: u64,
}

impl Default for PricesConfig {
    fn default() -> Self {
        Self {
            area: "FI".to_string(),
            currency: "EUR".to_string(),
            resolution_minutes: 60,
            vat_percent: 25.5,
            poll_start: NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            poll_interval_secs: 60,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub influxdb2: Option<InfluxDB2Config>,
    #[serde(default)]
    pub prices: PricesConfig,
}

pub fn read_config() -> Result<Config> {
    let builder = config::Config::builder();

    let root = std::env::current_dir()?;
    let sample_path = root.join("Settings.toml.example");

    let path = root.join("Settings.toml");

    if !path.exists() && std::env::var("SKIP_SAMPLE_CONFIG").is_err() {
        println!("Settings.toml not found, generating sample configuration.");
        println!("Set SKIP_SAMPLE_CONFIG environment variable to opt out of this behavior.");
        std::fs::copy(&sample_path, &path)
            .wrap_err_with(|| format!("Failed to copy {sample_path:?} to {path:?}"))?;
    }

    let builder = builder.add_source(config::File::with_name("Settings"));
    let settings = builder.build()?;

    let mut config: Config = settings.try_deserialize().wrap_err(eyre!(
        "Failed to deserialize config, compare your config file to Settings.toml.example!"
    ))?;

    if ![15, 60].contains(&config.prices.resolution_minutes) {
        bail!("prices.resolution_minutes must be either 15 or 60");
    }

    // The influxdb2 crate panics on invalid URLs, check them up front. Don't
    // refuse to start over this, publishing prices to MQTT is more important.
    if let Some(influx) = &config.influxdb2 {
        let valid = reqwest::Url::parse(&influx.host)
            .is_ok_and(|url| ["http", "https"].contains(&url.scheme()));

        if !valid {
            error!(
                "Invalid influxdb2.host {:?}, expected e.g. \"https://influxdb.example.com\". InfluxDB writes are disabled.",
                influx.host
            );
            config.influxdb2 = None;
        }
    }

    Ok(config)
}
