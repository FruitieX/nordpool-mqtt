FROM gcr.io/distroless/static@sha256:972618ca78034aaddc55864342014a96b85108c607372f7cbd0dbd1361f1d841
COPY target/x86_64-unknown-linux-musl/release/nordpool-mqtt /usr/local/bin/nordpool-mqtt
CMD ["nordpool-mqtt"]
