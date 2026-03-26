# Cthulu — pre-built binary + frontend copied into minimal runtime
# Agents run on user VMs via SSH, not locally.

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl sqlite3 openssl openssh-client \
 && update-ca-certificates \
 && rm -rf /var/lib/apt/lists/*

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs

COPY target/x86_64-unknown-linux-gnu/release/cthulu /usr/local/bin/cthulu
COPY cthulu-studio/dist /app/static/studio
COPY static/ /app/static/
COPY examples/prompts/ /app/prompts/

WORKDIR /app
RUN mkdir -p /data/.cthulu
ENV HOME=/data
ENV STORAGE=mongo
ENV MONGODB_DB=cthulu
ENV AUTH_ENABLED=true
ENV CTHULU_STATIC_DIR=/app/static
ENV PORT=8082

EXPOSE 8082
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
    CMD curl -sf http://localhost:8082/health || exit 1
CMD ["/usr/local/bin/cthulu", "serve"]
