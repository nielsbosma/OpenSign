# Deployment

OpenSign is one container image plus three things you provide: a Postgres database, an S3-compatible bucket, and (to accept Word files) a Gotenberg converter on a private network. Everything else is configured in the browser.

## What you need

| Part | |
| --- | --- |
| OpenSign | `ghcr.io/nielsbosma/opensign`. Multi-arch (amd64, arm64), distroless, runs as a non-root user, works with a read-only root file system. |
| Postgres | Version 15 or later. |
| S3 bucket | Any S3-compatible store: AWS S3, Cloudflare R2, MinIO, RustFS, a hosting provider's buckets. Files are encrypted before they reach it. |
| Gotenberg | Optional. `gotenberg/gotenberg:8`, on a private network with no internet access. Without it, only PDFs are accepted. |
| HTTPS | A public https address, usually from your platform or a reverse proxy. |

Email, SMS, eID, AI and every other provider are set up later in the browser.

### Image tags

| Tag | |
| --- | --- |
| `:latest` | The newest release |
| `:1.2` | The newest release in a series |
| `:1.2.3` | An exact release |
| `:edge` | The `main` branch, for testing |

## Environment variables

| Variable | |
| --- | --- |
| `DATABASE_URL` | Postgres connection string. |
| `S3_ENDPOINT`, `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY` | The bucket. |
| `S3_REGION` | Optional, default `us-east-1`. |
| `APP_SECRET` | At least 32 random characters, such as the output of `openssl rand -base64 48`. Every key derives from it. Keep it safe, back it up apart from the database, and never change it: without it every document is unreadable. |
| `APP_URL` | The public https address, such as `https://sign.example.com`. Used in links, passkeys and OAuth. |
| `CONVERTER_URL` | Gotenberg's private address, such as `http://gotenberg:3000`. Optional. |
| `SETUP_CODE` | Optional. At least 24 random characters that replace the setup code printed in the log, so a deployer can link straight to `/setup#code=…`. It stops working once the first admin exists. |
| `PORT` | Optional, default `3000`. |
| `ROLE` | Optional. `all` (default) runs the web server and the background worker; `web` or `worker` runs one half, to scale them apart. |

Any admin setting can also be pinned with an `OPENSIGN_<GROUP>_<FIELD>` variable. See [Admin settings](settings.md#pinning-settings).

## Sliplane

[Sliplane](https://sliplane.io) runs the whole stack with one click. The repository's `sliplane.yaml` describes it: a managed Postgres, a bucket, Gotenberg on the private network, and OpenSign with a freshly generated `APP_SECRET`. It asks for your Resend key (and optionally an AI provider) and shows what it creates and what it costs. When it finishes, **Open OpenSign** goes straight to setup with a generated `SETUP_CODE` filled in, so there is no code to find in the logs.

Sliplane keeps Gotenberg off the public internet for incoming traffic, but it has no setting that stops the service reaching out. If you need a converter without outbound access, as recommended under [Running it safely](#running-it-safely), run Gotenberg on an internal network of your own, or leave `CONVERTER_URL` unset to accept PDFs only.

**[Deploy to Sliplane](https://sliplane.opendeploy.cloud/?repo=https://github.com/nielsbosma/OpenSign)**

### From a terminal

The same setup through the Sliplane API:

```sh
SLIPLANE_TOKEN=api_rw_org_... ./deploy/sliplane.sh
```

The token needs read and write access to projects, servers, services, Postgres and buckets, and `secrets:read` so the database password can be read once. Nothing is written to disk. Options:

| Variable | |
| --- | --- |
| `OPENSIGN_TAG` | The image tag, such as `0.2.0`. Default `latest`. |
| `SLIPLANE_SERVER_ID` | The server to run on. Default: the only server, or the first one. |
| `SLIPLANE_PROJECT` | Project name. Default `OpenSign`. |
| `SLIPLANE_REGION` | Region for the database and bucket, such as `ger`, `fin` or `us-east`. Default `ger`. |
| `POSTGRES_INSTANCE` | Database size, such as `starter` or `base`. Default `starter`. |
| `DATABASE_URL`, `S3_*` | Bring your own database or bucket instead of creating them. |
| `RESEND_API_KEY`, `RESEND_FROM` | Pin email to Resend. Otherwise setup asks for it. |
| `AI_PROVIDER`, `AI_MODEL`, `AI_API_KEY` | Pin the optional AI provider. |

Run the script again to update. Afterwards, open the printed URL and enter the setup code from the service's logs.

## Docker

```sh
docker run -d --name opensign -p 3000:3000 \
  -e DATABASE_URL=postgres://opensign:secret@db:5432/opensign \
  -e S3_ENDPOINT=https://s3.example.com -e S3_BUCKET=opensign \
  -e S3_ACCESS_KEY=... -e S3_SECRET_KEY=... \
  -e APP_SECRET="$(openssl rand -base64 48)" \
  -e APP_URL=https://sign.example.com \
  -e CONVERTER_URL=http://gotenberg:3000 \
  ghcr.io/nielsbosma/opensign:latest
```

Generate `APP_SECRET` once and store it in your secret store, rather than generating it on each run as in this short example.

## Docker Compose

A minimal production compose file, with Gotenberg on an internal network:

```yaml
services:
  opensign:
    image: ghcr.io/nielsbosma/opensign:latest
    read_only: true
    tmpfs: [/tmp]
    ports: ["3000:3000"]
    env_file: opensign.env      # DATABASE_URL, S3_*, APP_SECRET, APP_URL
    environment:
      CONVERTER_URL: http://converter:3000
    networks: [default, converter]
    restart: unless-stopped

  converter:
    image: gotenberg/gotenberg:8
    command: [gotenberg, --api-timeout=60s, --chromium-disable-routes=true,
              --webhook-disable=true, --prometheus-disable-collect=true]
    networks: [converter]
    restart: unless-stopped

networks:
  converter:
    internal: true
```

Put a reverse proxy with TLS (Caddy, Traefik, nginx) in front, and use a managed Postgres and bucket or add them to the file. The repository's own `docker-compose.yml` is for development and testing only.

## Health checks

- `GET /api/health`: the process is up. Returns `{"ok":true,"version":"…"}`.
- `GET /api/ready`: the database answers. Returns `503` otherwise.
- The image has a built-in `HEALTHCHECK` (`/app/opensign health`).

## Database migrations

Every start applies pending migrations before serving, under a lock so only one container migrates. A build refuses to start against a database migrated by a newer version. See [Updates](updates.md).

## First run

Open `APP_URL` and enter the setup code from the container log. See [First-run setup and email](setup.md).

## Running it safely

- Serve OpenSign over https only, and set `APP_URL` to that address.
- Keep `APP_SECRET` in your platform's secret store, and back it up apart from the database.
- Keep Gotenberg on a private network without internet access. Sliplane cannot enforce the second part; see [Sliplane](#sliplane).
- Turn on **Require two-factor authentication** in **Settings → Security**.
- Back up Postgres and the bucket together. They are useless without each other, and both are useless without `APP_SECRET`.
- Leave outbound internet open for the providers you use (email, SMS, eID, time stamps, OCSP and CRLs, AI, webhooks, the GitHub update check).

See [Security model](security.md) for the full picture.
