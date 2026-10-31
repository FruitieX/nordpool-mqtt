use chrono::NaiveTime;
use eyre::{Context, Result};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Debug, Clone)]
pub struct MqttConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub topic: String,
    /// Broker credentials. The client connects anonymously when `username` is unset.
    pub username: Option<String>,
    pub password: Option<String>,
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

/// `[influxdb2]` (one target) or `[[influxdb2]]` (several, e.g. InfluxDB and
/// a TimescaleDB relay that accepts the InfluxDB v2 write API).
fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<InfluxDB2Config>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(InfluxDB2Config),
        Many(Vec<InfluxDB2Config>),
    }

    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(target) => vec![target],
        OneOrMany::Many(targets) => targets,
    })
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub mqtt: MqttConfig,
    /// InfluxDB v2 write targets; new prices are written to each of them.
    #[serde(default, deserialize_with = "one_or_many")]
    pub influxdb2: Vec<InfluxDB2Config>,
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

    parse_config(builder.add_source(config::File::with_name("Settings")))
}

fn parse_config(builder: config::ConfigBuilder<config::builder::DefaultState>) -> Result<Config> {
    let settings = builder.build()?;

    let mut config: Config = settings.try_deserialize().wrap_err(eyre!(
        "Failed to deserialize config, compare your config file to Settings.toml.example!"
    ))?;

    if ![15, 60].contains(&config.prices.resolution_minutes) {
        bail!("prices.resolution_minutes must be either 15 or 60");
    }

    // The influxdb2 crate panics on invalid URLs, check them up front. Don't
    // refuse to start over this, publishing prices to MQTT is more important.
    config.influxdb2.retain(|influx| {
        let valid = reqwest::Url::parse(&influx.host)
            .is_ok_and(|url| ["http", "https"].contains(&url.scheme()));

        if !valid {
            error!(
                "Invalid influxdb2.host {:?}, expected e.g. \"https://influxdb.example.com\". Writes to it are disabled.",
                influx.host
            );
        }

        valid
    });

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Result<Config> {
        parse_config(
            config::Config::builder()
                .add_source(config::File::from_str(toml, config::FileFormat::Toml)),
        )
    }

    #[test]
    fn parses_example_config() {
        let config = parse(include_str!("../Settings.toml.example")).unwrap();

        assert_eq!(config.mqtt.id, "nordpool-mqtt");
        assert_eq!(config.mqtt.port, 1883);
        assert_eq!(config.influxdb2.len(), 1);
        assert_eq!(config.influxdb2[0].bucket, "nordpool");
        assert_eq!(config.prices.area, "FI");
        assert_eq!(config.prices.resolution_minutes, 60);
        assert_eq!(
            config.prices.poll_start,
            NaiveTime::from_hms_opt(12, 0, 0).unwrap()
        );
    }

    #[test]
    fn prices_and_influxdb2_are_optional() {
        let config = parse(
            r#"
            [mqtt]
            id = "nordpool-mqtt"
            host = "localhost"
            port = 1883
            topic = "sensors/nordpool/price"
            "#,
        )
        .unwrap();

        assert!(config.influxdb2.is_empty());
        assert_eq!(config.prices.vat_percent, 25.5);
    }

    #[test]
    fn rejects_invalid_resolution() {
        let result = parse(
            r#"
            [mqtt]
            id = "nordpool-mqtt"
            host = "localhost"
            port = 1883
            topic = "sensors/nordpool/price"

            [prices]
            resolution_minutes = 30
            "#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn disables_influxdb2_with_invalid_host() {
        let config = parse(
            r#"
            [mqtt]
            id = "nordpool-mqtt"
            host = "localhost"
            port = 1883
            topic = "sensors/nordpool/price"

            [influxdb2]
            host = "influxdb.example.com"
            org = "org"
            token = "token"
            bucket = "bucket"
            "#,
        )
        .unwrap();

        assert!(config.influxdb2.is_empty());
    }

    #[test]
    fn parses_several_influxdb2_targets() {
        let config = parse(
            r#"
            [mqtt]
            id = "nordpool-mqtt"
            host = "localhost"
            port = 1883
            topic = "sensors/nordpool/price"

            [[influxdb2]]
            host = "https://influxdb.example.com"
            org = "org"
            token = "token"
            bucket = "nordpool"

            [[influxdb2]]
            host = "not a url"
            org = "org"
            token = "token"
            bucket = "nordpool"

            [[influxdb2]]
            host = "http://relay.example.com:8086"
            org = "org"
            token = "other-token"
            bucket = "nordpool"
            "#,
        )
        .unwrap();

        let hosts: Vec<_> = config.influxdb2.iter().map(|t| t.host.as_str()).collect();
        assert_eq!(
            hosts,
            [
                "https://influxdb.example.com",
                "http://relay.example.com:8086"
            ]
        );
    }
}
