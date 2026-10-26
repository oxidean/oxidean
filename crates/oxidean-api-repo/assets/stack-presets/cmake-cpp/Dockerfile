# syntax=docker/dockerfile:1

FROM debian:bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake g++ make \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cmake -B build -DCMAKE_BUILD_TYPE=Release \
    && cmake --build build

FROM debian:bookworm-slim
COPY --from=build /app/build/app /usr/local/bin/app
CMD ["/usr/local/bin/app"]
