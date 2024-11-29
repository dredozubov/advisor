# syntax=docker/dockerfile:1.4

# 1. Base stage - Common dependencies
FROM rust:1.82-slim-bookworm as base
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# 2. Development dependencies stage
FROM base as dev-deps
RUN apt-get update && apt-get install -y \
    python3-pip \
    python3-venv \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 3. Builder stage - Compile the application
FROM base as builder
WORKDIR /build
COPY Cargo.* ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target

COPY . .
RUN cargo build --release

# 4. Development stage
FROM dev-deps as development
ARG USERNAME=vscode
ARG USER_UID=1000
ARG USER_GID=$USER_UID

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH

# Create non-root user
RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && mkdir -p /usr/local/cargo /workspace \
    && chown -R $USERNAME:$USERNAME /usr/local/cargo /workspace \
    && chmod -R g+w /usr/local/cargo

# Setup Python environment
RUN python3 -m venv /usr/local/aider-venv && \
    /usr/local/aider-venv/bin/pip install aider-chat && \
    chown -R $USERNAME:$USERNAME /usr/local/aider-venv

USER $USERNAME
WORKDIR /workspace

# Install development tools
RUN cargo install cargo-watch cargo-edit cargo-audit cargo-deny cargo-outdated sqlx-cli

# Install clippy
RUN rustup component add clippy

# 5. Production stage
FROM debian:bookworm-slim as production

ARG USERNAME=advisor
ARG USER_UID=1000
ARG USER_GID=$USER_UID

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME

# Copy binary from builder
COPY --from=builder --chown=$USERNAME:$USERNAME /build/target/release/advisor /usr/local/bin/
COPY --from=builder --chown=$USERNAME:$USERNAME /build/config /etc/advisor/config

# Create necessary directories
RUN mkdir -p /var/lib/advisor/data \
    && chown -R $USERNAME:$USERNAME /var/lib/advisor

USER $USERNAME
WORKDIR /var/lib/advisor

# Health check
HEALTHCHECK --interval=30s --timeout=30s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

EXPOSE 3000
CMD ["advisor"]
