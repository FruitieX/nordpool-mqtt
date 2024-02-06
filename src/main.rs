use croner::Cron;
use eyre::Result;
use state::State;

#[macro_use]
extern crate eyre;

#[macro_use]
extern crate log;

mod api;
mod config;
mod mqtt;
mod state;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    color_eyre::install()?;

    let config = config::read_config()?;
    let mqtt_client = mqtt::init_mqtt(&config)?;
    let cron = Cron::new("0 * * * *")
        .parse()
        .expect("Invalid cron expression");

    let mut state = State::new(config, cron, mqtt_client);

    loop {
        let result = state.process().await;

        if let Err(e) = result {
            error!("Error: {}", e);
        }
    }
}
