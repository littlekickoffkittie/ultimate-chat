FROM rust:1.83-slim as builder
WORKDIR /usr/src/app
COPY . .
# We remove the lockfile to ensure Cargo builds fresh with the container's version
RUN rm -f Cargo.lock && cargo build --release --bin server

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/server /app/server
EXPOSE 8080
CMD ["./server"]
