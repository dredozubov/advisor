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
WORKDIR /workspace