use eyre::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct MqttConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub topic: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub mqtt: MqttConfig,
}

pub fn read_config() -> Result<MqttConfig> {
    let builder = config::Config::builder();

    let root = std::env::current_dir().unwrap();
    let sample_path = root.join("Settings.toml.example");

    let path = root.join("Settings.toml");

    if !path.exists() && std::env::var("SKIP_SAMPLE_CONFIG").is_err() {
        println!("Settings.toml not found, generating sample configuration.");
        println!("Set SKIP_SAMPLE_CONFIG environment variable to opt out of this behavior.");
        std::fs::copy(sample_path, path).unwrap();
    }

    let builder = builder.add_source(config::File::with_name("Settings"));
    let settings = builder.build()?;

    let config: Config = settings.try_deserialize().wrap_err(eyre!(
        "Failed to deserialize config, compare your config file to Settings.toml.example!"
    ))?;

    let mqtt_config = config.mqtt;

    Ok(mqtt_config)
}
