# syntax=docker/dockerfile:1.12@sha256:93bfd3b68c109427185cd78b4779fc82b484b0b7618e36d0f104d4d801e66d25
ARG RUST_IMAGE=rust:1.94-bookworm@sha256:6ae102bdbf528294bc79ad6e1fae682f6f7c2a6e6621506ba959f9685b308a55
ARG RUNTIME_IMAGE=debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

FROM ${RUST_IMAGE} AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
COPY scripts/render_artwork_label.swift ./scripts/render_artwork_label.swift
COPY web ./web
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/build/target,sharing=locked \
    cargo build --locked --release --bin chordrift-server --bin chordrift-worker \
      --bin chordrift-hosted-plan-apply --bin chordrift-reviewed-import \
      --bin chordrift-reviewed-cadence && \
    cp /build/target/release/chordrift-server /tmp/chordrift-server && \
    cp /build/target/release/chordrift-worker /tmp/chordrift-worker && \
    cp /build/target/release/chordrift-hosted-plan-apply /tmp/chordrift-hosted-plan-apply && \
    cp /build/target/release/chordrift-reviewed-import /tmp/chordrift-reviewed-import && \
    cp /build/target/release/chordrift-reviewed-cadence /tmp/chordrift-reviewed-cadence

FROM ${RUNTIME_IMAGE} AS runtime
ARG VCS_REF=unknown
ARG BUILD_DATE=unknown
LABEL org.opencontainers.image.title="Chordrift hosted authority" \
      org.opencontainers.image.source="https://github.com/orbyts/chordrift" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.created="${BUILD_DATE}"
RUN apt-get update && \
    apt-get install --yes --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --system --gid 65532 chordrift && \
    useradd --system --uid 65532 --gid 65532 --no-create-home --home-dir /nonexistent chordrift
COPY --from=builder --chmod=0555 /tmp/chordrift-server /usr/local/bin/chordrift-server
COPY --from=builder --chmod=0555 /tmp/chordrift-worker /usr/local/bin/chordrift-worker
COPY --from=builder --chmod=0555 /tmp/chordrift-hosted-plan-apply /usr/local/bin/chordrift-hosted-plan-apply
COPY --from=builder --chmod=0555 /tmp/chordrift-reviewed-import /usr/local/bin/chordrift-reviewed-import
COPY --from=builder --chmod=0555 /tmp/chordrift-reviewed-cadence /usr/local/bin/chordrift-reviewed-cadence
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/chordrift-server"]
