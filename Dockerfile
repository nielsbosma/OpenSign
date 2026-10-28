# One image for the whole app: web app, API, MCP and the job worker, as a single static-ish binary
# on a distroless base (no shell, no package manager), running as a non-root user.
#   docker run -p 3000:3000 -e DATABASE_URL=... -e S3_BUCKET=... -e S3_ACCESS_KEY=... -e S3_SECRET_KEY=... \
#     -e APP_SECRET=... -e APP_URL=https://sign.example.com ghcr.io/nielsbosma/opensign
# ROLE=web or ROLE=worker runs only one half; the default runs both.

FROM node:24-alpine AS web
ENV PNPM_HOME=/pnpm PATH=/pnpm:$PATH COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY web/package.json web/
RUN --mount=type=cache,id=pnpm,target=/pnpm/store pnpm install --frozen-lockfile --filter @opensign/web
COPY web web
RUN pnpm --filter @opensign/web build

FROM rust:1.99-slim AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
# Compiled into the binary. Set by CI from the release tag; empty means a local build.
ARG OPENSIGN_VERSION=
ENV OPENSIGN_VERSION=$OPENSIGN_VERSION
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin opensign && cp target/release/opensign /opensign

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=server /opensign /app/opensign
COPY --from=web /src/web/dist /app/web
ENV PORT=3000 WEB_DIST=/app/web APP_ENV=production
USER nonroot
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD ["/app/opensign", "health"]
ENTRYPOINT ["/app/opensign"]
