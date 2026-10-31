# nordpool-mqtt

This program reads day-ahead electricity prices from the [Nord Pool](https://data.nordpoolgroup.com/) data portal API and publishes the current price to an MQTT broker.

Prices are published as cents per kWh (or 1/100 of the configured currency) with VAT included. With the default settings (area `FI`, 60 minute resolution, 25.5% VAT) the values are identical to what https://porssisahko.net/api reports.

## How it works

- On startup, today's prices are fetched (retrying with backoff until that succeeds), and the current price is published right away.
- A new price is published at the start of every price period (every hour, or every 15 minutes with `resolution_minutes = 15`).
- From `prices.poll_start` (12:00 CET by default) onwards, the API is polled every `prices.poll_interval_secs` (60 s by default) until the next day's final prices are available. Nord Pool usually publishes them around 12:45 CET.
- The latest price is republished whenever the MQTT connection is (re-)established, so the retained message survives broker restarts.
- If prices change (e.g. preliminary prices getting corrected), the new prices are fetched and published.
- New prices are optionally written to InfluxDB 2, or to several InfluxDB v2 write endpoints (`[[influxdb2]]` once per target, e.g. InfluxDB plus a Telegraf `influxdb_v2_listener` in front of TimescaleDB). Each target is written independently.

Set `RUST_LOG` to adjust log verbosity (defaults to `info`).

## Setup

### Development

Run `nix develop` (or use direnv) to get a shell with the Rust toolchain.

### Quick start

- Copy `Settings.toml.example` to `Settings.toml`.
- Edit `Settings.toml` with values matching your setup.

### Setting Up Mosquitto

- Ensure Docker is installed and running

- Run mosquitto (with authentication disabled) using:

  ```
  docker run -it -p 1883:1883 eclipse-mosquitto mosquitto -c /mosquitto-no-auth.conf
  ```

### Setting Up MQTT Explorer

- Install [MQTT Explorer](http://mqtt-explorer.com/)

- Connect to the MQTT broker by configuring the connection

  - Setting the protocol to mqtt://
  - Setting the host to localhost
  - Setting the port to 1883

## Topics

The price will be published to the topic configured under `mqtt.topic` in `Settings.toml`.

## State messages

For historic reasons I'm including some redundant fields in the published JSON, subject to changes if/when my home automation server supports other formats:

```
{
  "id": "nordpool-mqtt",
  "name": "nordpool-mqtt",
  "price": 19.219,
  "sensor_value": 19.219,
  "start_date": "2024-02-06T18:00:00+00:00",
  "end_date": "2024-02-06T19:00:00+00:00"
}
```
