FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Useful native deps
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates curl jq && \
    rm -rf /var/lib/apt/lists/*
