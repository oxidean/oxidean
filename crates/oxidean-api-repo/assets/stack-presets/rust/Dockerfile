# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/target/release/app /usr/local/bin/app
CMD ["/usr/local/bin/app"]
