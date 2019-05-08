FROM rust:1.34-stretch

COPY . /src
WORKDIR /src

RUN cargo build

FROM debian:stretch

RUN apt-get update
RUN apt-get install -y libssl-dev libsqlite3-0

WORKDIR /root/
COPY --from=0 /src/target/debug/shaft .
COPY --from=0 /src/res/ res/

ENTRYPOINT ["./shaft"]
