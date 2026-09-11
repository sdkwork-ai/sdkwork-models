FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p sdkwork-api-models-standalone-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /src/target/release/sdkwork-api-models-standalone-gateway /usr/local/bin/sdkwork-api-models-standalone-gateway
COPY --from=builder /src/database /app/database
ENV SDKWORK_MODELS_APPLICATION_PUBLIC_INGRESS_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["sdkwork-api-models-standalone-gateway"]
