FROM rust:latest

COPY . /src
WORKDIR /src

RUN cargo build

FROM debian:stretch

RUN apt-get update
RUN apt-get update && apt-get install -y libssl-dev libsqlite3-0

WORKDIR /root/
COPY --from=0 /src/target/debug/shaft .
COPY --from=0 /src/res/ res/

RUN apt-get update && apt-get install -y ca-certificates openssl

ENTRYPOINT ["./shaft"]
