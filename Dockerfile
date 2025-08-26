# Use Debian for building to avoid Alpine + jemalloc issues
FROM --platform=$BUILDPLATFORM rust:1.89-bookworm AS builder

# Build arguments for cross-compilation
ARG TARGETPLATFORM
ARG BUILDPLATFORM

# Install dependencies for building and cross-compilation
RUN apt update && apt install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    musl-tools \
    gcc-aarch64-linux-gnu \
    && apt clean \
    && rm -rf /var/lib/apt/lists/*

# Add MUSL targets for both architectures
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

# Set up cross-compilation environment
ENV CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc

# Set working directory
WORKDIR /usr/src/envx

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build the application for the target platform
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") \
        cargo build --release --target x86_64-unknown-linux-musl \
        ;; \
    "linux/arm64") \
        cargo build --release --target aarch64-unknown-linux-musl \
        ;; \
    *) \
        echo "Unsupported platform: $TARGETPLATFORM" && exit 1 \
        ;; \
    esac

# Copy the correct binary based on target platform
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") \
        cp /usr/src/envx/target/x86_64-unknown-linux-musl/release/envx /usr/local/bin/envx \
        ;; \
    "linux/arm64") \
        cp /usr/src/envx/target/aarch64-unknown-linux-musl/release/envx /usr/local/bin/envx \
        ;; \
    esac

# Runtime stage - minimal distroless image (supports multi-arch)
FROM gcr.io/distroless/static-debian12:nonroot

# Copy binary from builder stage
COPY --from=builder /usr/local/bin/envx /usr/local/bin/envx

# Create directory for vault data (distroless already has nonroot user)
USER 65532:65532
WORKDIR /home/nonroot

ENTRYPOINT ["/usr/local/bin/envx"]
CMD ["--help"]