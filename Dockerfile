# syntax=docker/dockerfile:1.7

ARG ALPINE_IMAGE=alpine:3.24@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b
ARG NODE_IMAGE=node:24-alpine3.24@sha256:a0b9bf06e4e6193cf7a0f58816cc935ff8c2a908f81e6f1a95432d679c54fbfd
ARG RUST_IMAGE=rust:alpine3.24@sha256:f87aa870663e2b57ec8c69de82c7eedf7383bee987eef7612c0359635eaadb41

FROM --platform=$BUILDPLATFORM ${NODE_IMAGE} AS web-deps
WORKDIR /src/clients/web
COPY clients/web/package.json clients/web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm \
    npm ci

FROM web-deps AS web-builder
COPY clients/web ./
RUN npm run build

FROM --platform=$TARGETPLATFORM ${RUST_IMAGE} AS rust-builder
ARG TARGETARCH
WORKDIR /src
RUN case "$TARGETARCH" in amd64|arm64) ;; *) echo "Unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; esac \
    && apk add --no-cache \
        build-base \
        clang \
        cmake \
        linux-headers \
        perl \
        pkgconf \
        protobuf-dev
COPY Cargo.toml Cargo.lock ./
COPY vendor ./vendor
COPY crates ./crates
COPY server ./server
COPY clients/desktop/src-tauri ./clients/desktop/src-tauri
COPY native/ffmpeg-bootstrap/bootstrap.c ./native/ffmpeg-bootstrap/bootstrap.c
RUN cc -std=c11 -O2 -fPIC -fstack-protector-strong -Wall -Wextra -Werror -shared \
        -Wl,-z,now,-z,relro,-soname,/usr/local/lib/duskcue-ffmpeg-bootstrap.so \
        -o /tmp/duskcue-ffmpeg-bootstrap.so /src/native/ffmpeg-bootstrap/bootstrap.c
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p duskcue -j 2 \
    && cp target/release/duskcue /tmp/duskcue

FROM ${ALPINE_IMAGE} AS runtime
ARG BUILD_DATE=unknown
ARG VCS_REF=unknown
ARG VERSION=0.1.0
LABEL org.opencontainers.image.title="Duskcue" \
      org.opencontainers.image.description="Self-hosted media streaming server" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.licenses="AGPL-3.0"
RUN apk add --no-cache \
        bash \
        ca-certificates \
        curl \
        ffmpeg \
        font-noto \
        font-noto-arabic \
        font-noto-cjk \
        font-noto-devanagari \
        font-noto-hebrew \
        font-noto-thai \
        libgcc \
        libstdc++ \
        nodejs \
        nss_wrapper \
        postgresql18 \
        postgresql18-client \
        postgresql18-contrib \
        su-exec \
        tini \
        tzdata \
    && addgroup -S duskcue \
    && adduser -S -D -H -h /data -s /sbin/nologin -G duskcue duskcue \
    && mkdir -p \
        /cache \
        /data \
        /data/config \
        /data/transcode \
        /media \
        /opt/duskcue/web \
        /var/lib/postgresql \
        /var/run/postgresql \
    && chown -R duskcue:duskcue \
        /cache \
        /data \
        /media \
        /opt/duskcue \
        /var/lib/postgresql \
        /var/run/postgresql \
    && (find / -xdev -type f -perm /6000 -exec chmod a-s {} + 2>/dev/null || true)
WORKDIR /opt/duskcue
COPY docker/fonts/60-duskcue-sans-fallback.conf /etc/fonts/conf.d/60-duskcue-sans-fallback.conf
COPY --from=rust-builder /tmp/duskcue /usr/local/bin/duskcue
COPY --from=rust-builder /tmp/duskcue-ffmpeg-bootstrap.so /usr/local/lib/duskcue-ffmpeg-bootstrap.so
COPY --from=web-builder /src/clients/web/build /opt/duskcue/web
COPY docker/entrypoint.sh /usr/local/bin/duskcue-entrypoint
RUN chmod 0755 /usr/local/bin/duskcue /usr/local/bin/duskcue-entrypoint \
    && chown -R duskcue:duskcue /opt/duskcue
RUN apk add --no-cache --virtual .ffmpeg-bootstrap-tools patchelf \
    && mkdir -p /usr/local/libexec \
    && cp /usr/bin/ffmpeg /usr/local/libexec/duskcue-ffmpeg \
    && patchelf --add-needed /usr/local/lib/duskcue-ffmpeg-bootstrap.so /usr/local/libexec/duskcue-ffmpeg \
    && chmod 0555 /usr/local/libexec/duskcue-ffmpeg /usr/local/lib/duskcue-ffmpeg-bootstrap.so \
    && sha256sum /usr/local/libexec/duskcue-ffmpeg /usr/local/lib/duskcue-ffmpeg-bootstrap.so > /usr/local/libexec/duskcue-ffmpeg.sha256 \
    && chmod 0444 /usr/local/libexec/duskcue-ffmpeg.sha256 \
    && apk del .ffmpeg-bootstrap-tools
ENV DUSKCUE_DATA_DIR=/data \
    DUSKCUE_CACHE_DIR=/cache \
    DUSKCUE_ENVIRONMENT=production \
    DUSKCUE_INTERNAL_BIND_ADDRESS=127.0.0.1 \
    DUSKCUE_INTERNAL_API_PORT=48028 \
    NODE_ENV=production
EXPOSE 48027
VOLUME ["/data", "/cache"]
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl --fail --silent --max-time 5 http://127.0.0.1:48027/health/ready >/dev/null || exit 1
ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/duskcue-entrypoint"]
CMD ["start"]
