# nordpool-mqtt

This program reads current Finnish electricity exchange prices from https://porssisahko.net/api and publishes them to an MQTT broker.

## Setup

### Quick start

- Copy `Settings.example.toml` to `Settings.toml`.
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
  "id":"nordpool-mqtt",
  "name":"nordpool-mqtt",
  "price":19.219,
  "sensor_value":19.219,"start_date":"2024-02-06T18:00:00+00:00"
}
```
