FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

COPY target/release/cthulu /usr/local/bin/cthulu

EXPOSE 8081
CMD ["cthulu"]