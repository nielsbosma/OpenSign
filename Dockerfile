# One image for the whole app: web app, API, MCP and the job worker, as a single static-ish binary
# on a distroless base (no shell, no package manager), running as a non-root user.
#   docker run -p 3000:3000 -e DATABASE_URL=... -e S3_BUCKET=... -e S3_ACCESS_KEY=... -e S3_SECRET_KEY=... \
#     -e APP_SECRET=... -e APP_URL=https://sign.example.com ghcr.io/nielsbosma/opensign
# ROLE=web or ROLE=worker runs only one half; the default runs both.

FROM node:26-alpine@sha256:8996bb5821d62a8fd9bb27c1cb07462f3fdce6e47d640c6a881ead0b064ce216 AS web
ENV PNPM_HOME=/pnpm PATH=/pnpm:$PATH COREPACK_ENABLE_DOWNLOAD_PROMPT=0
# Node 25 and later no longer ship corepack.
RUN npm install -g corepack@0.36.0 && corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY web/package.json web/
RUN --mount=type=cache,id=pnpm,target=/pnpm/store pnpm install --frozen-lockfile --filter @opensign/web
COPY web web
COPY docs docs
RUN pnpm --filter @opensign/web build

FROM rust:1.99-slim@sha256:f6c4a35d21dbb3c89d7a8d056c193e94fc09037f2b58dd858b09c39a47dd02d7 AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY docs docs
# Compiled into the binary. Set by CI from the release tag; empty means a local build.
ARG OPENSIGN_VERSION=
ENV OPENSIGN_VERSION=$OPENSIGN_VERSION
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin opensign && cp target/release/opensign /opensign

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:532c7a94f9e96ba9db75eb9388f4227751131c7cb0a411d1a8521af4096135de
COPY --from=server /opensign /app/opensign
COPY --from=web /src/web/dist /app/web
ENV PORT=3000 WEB_DIST=/app/web APP_ENV=production
USER nonroot
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD ["/app/opensign", "health"]
ENTRYPOINT ["/app/opensign"]
