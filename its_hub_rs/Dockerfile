FROM rust:1.87-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src
COPY src/ src/
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/its-hub-rs /usr/local/bin/its-hub-rs
EXPOSE 8108
ENTRYPOINT ["/usr/local/bin/its-hub-rs"]
CMD ["--host", "0.0.0.0", "--port", "8108"]
