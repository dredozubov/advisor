FROM rust:1.75-slim-bookworm

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
ARG USERNAME=app
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && chown -R $USERNAME:$USERNAME /app

USER $USERNAME

# Copy source code
COPY --chown=$USERNAME:$USERNAME . .

# Build the application
RUN cargo build --release

# Run the application
CMD ["./target/release/advisor"]
