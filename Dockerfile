FROM gcr.io/distroless/static@sha256:b7b9a6953e7bed6baaf37329331051d7bdc1b99c885f6dbeb72d75b1baad54f9
COPY target/x86_64-unknown-linux-musl/release/nordpool-mqtt /usr/local/bin/nordpool-mqtt
CMD ["nordpool-mqtt"]
