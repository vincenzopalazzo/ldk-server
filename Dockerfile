FROM rust:1.85 AS builder

WORKDIR /app
ARG ENABLE_LSPS2=false
ARG GIT_HASH
ENV GIT_HASH=$GIT_HASH

# Copy manifests and lock file first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY ldk-server/Cargo.toml ldk-server/Cargo.toml
COPY ldk-server-cli/Cargo.toml ldk-server-cli/Cargo.toml
COPY ldk-server-client/Cargo.toml ldk-server-client/Cargo.toml
COPY ldk-server-grpc/Cargo.toml ldk-server-grpc/Cargo.toml
COPY ldk-server-grpc/build.rs ldk-server-grpc/build.rs
COPY ldk-server-mcp/Cargo.toml ldk-server-mcp/Cargo.toml

# Create dummy source files so cargo can resolve and build dependencies
RUN mkdir -p ldk-server/src ldk-server-cli/src ldk-server-client/src ldk-server-grpc/src ldk-server-mcp/src \
    && echo "fn main() {}" > ldk-server/src/main.rs \
    && echo "fn main() {}" > ldk-server-cli/src/main.rs \
    && echo "" > ldk-server-client/src/lib.rs \
    && echo "" > ldk-server-grpc/src/lib.rs \
    && echo "fn main() {}" > ldk-server-mcp/src/main.rs

# Build dependencies only (this layer is cached unless Cargo.toml/Cargo.lock change)
RUN if [ "$ENABLE_LSPS2" = "true" ]; then \
        cargo build --release --locked --features experimental-lsps2-support \
            -p ldk-server \
            -p ldk-server-cli \
            -p ldk-server-mcp; \
    else \
        cargo build --release --locked \
            -p ldk-server \
            -p ldk-server-cli \
            -p ldk-server-mcp; \
    fi

# Copy real source and rebuild
COPY . .
RUN touch ldk-server/src/main.rs ldk-server-cli/src/main.rs ldk-server-mcp/src/main.rs \
    ldk-server-client/src/lib.rs ldk-server-grpc/src/lib.rs \
    && if [ "$ENABLE_LSPS2" = "true" ]; then \
        cargo build --release --locked --features experimental-lsps2-support \
            -p ldk-server \
            -p ldk-server-cli \
            -p ldk-server-mcp; \
    else \
        cargo build --release --locked \
            -p ldk-server \
            -p ldk-server-cli \
            -p ldk-server-mcp; \
    fi

# The MCP server as its own image: `docker build --target ldk-server-mcp .`
# It speaks MCP over stdio, so run it with `docker run -i` and point it at a node with
# LDK_BASE_URL, LDK_API_KEY and LDK_TLS_CERT_PATH.
FROM debian:bookworm-slim AS ldk-server-mcp

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ldk-server-mcp /usr/local/bin/ldk-server-mcp

ENTRYPOINT ["ldk-server-mcp"]

# The server, the default target: `docker build .`
FROM debian:bookworm-slim AS ldk-server

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ldk-server /usr/local/bin/ldk-server
COPY --from=builder /app/target/release/ldk-server-cli /usr/local/bin/ldk-server-cli

ENV LDK_SERVER_NODE_GRPC_SERVICE_ADDRESS=0.0.0.0:3536

EXPOSE 9735 3536

ENTRYPOINT ["ldk-server"]
