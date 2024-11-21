FROM rust:1.75-slim-bookworm

# Create non-root user
RUN useradd -m -s /bin/bash vscode \
    && mkdir -p /workspace \
    && chown -R vscode:vscode /workspace

# Set up cargo permissions
RUN mkdir -p /usr/local/cargo \
    && chown -R vscode:vscode /usr/local/cargo \
    && chmod -R 775 /usr/local/cargo

# Set cargo env vars
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH=/usr/local/cargo/bin:$PATH

# Switch to non-root user
USER vscode
WORKDIR /workspaceFROM rust:1.74-slim-bookworm

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Pre-build dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Build actual application
COPY . .
RUN cargo build --release

CMD ["/workspace/target/release/advisor"]
