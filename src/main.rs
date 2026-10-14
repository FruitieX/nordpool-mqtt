use eyre::Result;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;

#[macro_use]
extern crate eyre;

#[macro_use]
extern crate log;

mod api;
mod config;
mod influxdb2;
mod mqtt;
mod prices;
mod publisher;
mod time;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::formatted_timed_builder()
        .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();
    color_eyre::install()?;

    let config = config::read_config()?;
    let (mqtt, mut mqtt_task) = mqtt::init_mqtt(&config.mqtt);
    let (tx, rx) = watch::channel(prices::PriceStore::default());

    let mut fetcher = tokio::spawn(prices::run_fetcher(config, tx));
    let mut publisher = tokio::spawn(publisher::run_publisher(rx, mqtt));

    let mut sigterm = signal(SignalKind::terminate())?;

    // None of these should ever finish. If one does, exit so that whatever
    // supervises us restarts the process, instead of silently running on in
    // a half-broken state.
    tokio::select! {
        res = &mut fetcher => bail!("Price fetcher stopped: {res:?}"),
        res = &mut publisher => bail!("Price publisher stopped: {res:?}"),
        res = &mut mqtt_task => bail!("MQTT event loop stopped: {res:?}"),
        _ = tokio::signal::ctrl_c() => info!("Received SIGINT, exiting"),
        _ = sigterm.recv() => info!("Received SIGTERM, exiting"),
    }

    Ok(())
}
