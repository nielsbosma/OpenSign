# OpenSign

An open source electronic signing service you run yourself. Upload a Word or PDF document, place fields, send it, and get back a sealed PDF that any PDF reader can verify, with an evidence report of who signed, when, and how they proved who they were.

- **Signing flows**: signers, approvers and viewers, in order or all at once; email, SMS or link delivery, or in person on your own device; reminders, expiry, decline with a reason; templates. Signers can be asked for attachments (an ID copy, a certificate), which are cleaned and sealed into the signed PDF. Signers see everything in English or Swedish.
- **Bulk send**: send a template to every row of a CSV or Excel file. Columns are matched to parties and fields by name (Swedish or English), optionally with AI, and every row is checked before anything is sent.
- **Identification**: email link, SMS code (46elks or Twilio), Swedish BankID (with your own agreement or through TIC Identity), Freja eID, and other Nordic eIDs through an OpenID Connect broker (Idura/Criipto, Signicat). With a personal number on the party, only that person can sign.
- **Sealed PDFs**: PAdES B-LTA: a seal over the whole file with an RFC 3161 time stamp, the certificates and OCSP or CRL answers needed to validate it offline, and an archive time stamp over everything. Evidence pages, and a verify page and API that check any OpenSign document and flag any change made after a signature other than validation data.
- **Word and PDF uploads**: Word, ODT and RTF are converted by Gotenberg on a private network; every PDF is sanitized (no JavaScript, forms actions, embedded files or XFA) before anyone sees it.
- **APIs**: a REST API with OpenAPI, plus drop-in compatible endpoints for **Scrive** (`/scrive/api/v2`) and **DocuSign** eSignature (`/docusign/restapi/v2.1`), signed webhooks, and idempotency keys.
- **MCP**: everything you can do in the app is available to Claude and other MCP clients, with your permissions and in the audit trail.
- **AI (optional)**: an assistant over your own tools, and an LLM review of a document before you send it. Off until an admin picks a provider; document text is only sent if an admin allows it.
- **Retention and legal hold**: rules in Settings delete signed, ended and draft documents with their files after set periods; a legal hold keeps a document until it is released.
- **Multi-user with roles**: admin, manager, sender, auditor and viewer; passkeys or an authenticator app for two-factor. One organisation per install.
- **Security first**: see [SECURITY.md](SECURITY.md) for the threat model and the controls.

## Running it

OpenSign is one container image plus Postgres, an S3-compatible bucket and (for Word files) Gotenberg.

| Variable | |
| --- | --- |
| `DATABASE_URL` | Postgres 15 or later |
| `S3_ENDPOINT`, `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, `S3_REGION` | The bucket; documents are encrypted before they reach it |
| `APP_SECRET` | 32+ random characters, e.g. `openssl rand -base64 48`. Every key derives from it: keep it safe and never change it |
| `APP_URL` | The public https address |
| `CONVERTER_URL` | Gotenberg, on a private network (optional; without it only PDFs are accepted) |

Everything else (Resend, SMS, eID, AI, security rules, branding) is set in the browser.

### On Sliplane

[![Deploy to Sliplane](https://sliplane.opendeploy.cloud/button.svg)](https://sliplane.opendeploy.cloud/?repo=https://github.com/nielsbosma/OpenSign)

The button reads [sliplane.yaml](sliplane.yaml): it asks for your Resend key (and optionally an AI provider), shows what it will create and what it costs, and sets up a managed Postgres, a bucket, Gotenberg on the private network and OpenSign with a fresh `APP_SECRET`. The deployed app links to its own updates.

From a terminal, the same thing over the [Sliplane API](https://ctrl.sliplane.io):

```sh
SLIPLANE_TOKEN=api_rw_org_... ./deploy/sliplane.sh
```

Run it again to update (`OPENSIGN_TAG=0.2.0` for a release). See the top of the script for options, such as bringing your own database.

### Anywhere else

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

The image is distroless, runs as a non-root user, works with a read-only root file system, and is multi-arch (amd64, arm64). Tags: `:latest`, `:1.2.3` and `:1.2` for releases, `:edge` for `main`.

### First run

1. The container log prints a **setup code**, so nobody else can claim a fresh install.
2. Create the first admin.
3. Enter a **Resend** API key and sender address. OpenSign sends a test email and only continues once Resend accepted it; documents cannot go out by email until then.
4. Optionally set up SMS, eID, your seal certificate and AI in **Admin settings**.

Any admin setting can be pinned with an environment variable, `OPENSIGN_<GROUP>_<FIELD>`, for example `OPENSIGN_EMAIL_API_KEY` or `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR=true`. Pinned fields are locked in the UI.

### Updating

**Settings → System** shows the running version, the latest release on GitHub, the database migrations and the job queue. Admins with an update waiting also see **Update to X** in the sidebar.

Every start applies pending database migrations before serving, under a lock so only one container migrates. A build refuses to start against a database migrated by a newer one. So updating is only ever "run the newer image":

- **From the app**: put your Sliplane deploy hook (Sliplane → service → Settings → Deploy hook) in **Settings → Updates**, then press **Update Now** on the System page. The hook redeploys the service's image tag, so run `:latest` or a release series such as `:1.2`, not an exact version.
- **Sliplane from your machine**: `OPENSIGN_TAG=1.2.0 ./deploy/sliplane.sh`.
- **Docker Compose**: `docker compose pull && docker compose up -d`.

Back up the database before a major version. Turn off the GitHub check in **Settings → Updates** for installs without internet access.

### eID providers

- **Swedish BankID**: turn it on with **Use the BankID test environment** and it works right away with BankID's public test certificate and a [test BankID](https://developers.bankid.com/test-portal). For production, upload the `.p12` your bank gives you and type its password; BankID's server root is built in.
- **Freja eID**: upload the `.pfx` from Freja with its password, and Freja's result-signing certificate. Results whose signature does not check out are refused.
- **Broker**: issuer URL, client id and secret. Register `https://<your-install>/api/eid/callback` as the redirect URI. `acr_values` picks a specific eID.
- **BankID through [TIC Identity](https://id.tic.io)**: for BankID without an agreement with a bank. Turn on OIDC for your tenant in TIC's portal, register `https://<your-install>/api/eid/callback` as the redirect URI, and enter the tenant name, client id and secret. Parties still choose Swedish BankID; it goes through TIC whenever BankID with your own certificate is off.

## Using the API

Create an API key under **Settings → API Keys**, then:

```sh
curl -H "Authorization: Bearer osk_..." https://sign.example.com/api/v1/documents
```

The **API Explorer** in the sidebar lists every endpoint with its parameters and schemas, sends live requests as you or with a pasted API key, and writes the same call as curl, JavaScript or Python. The OpenAPI document behind it is at `/api/v1/openapi.json`. Existing Scrive or DocuSign integrations can point at OpenSign by swapping the base URL and using an OpenSign API key as the bearer token (Scrive's OAuth header takes it as `oauth_token`):

| API | Vendor base URL | OpenSign base URL |
| --- | --- | --- |
| Scrive v2 | `https://scrive.com/api/v2` | `https://sign.example.com/scrive/api/v2` |
| DocuSign eSignature v2.1 | `https://demo.docusign.net/restapi/v2.1` | `https://sign.example.com/docusign/restapi/v2.1` |

The older `/api/v2` and `/restapi/v2.1` paths still work as aliases.

## Connecting Claude

In Claude, add a custom connector with `https://<your-install>/mcp` (also shown under **Settings → Account**), sign in and approve. It uses OAuth 2.1 with dynamic client registration and PKCE, over Streamable HTTP.

## Architecture

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Resend, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

Rust (axum, sqlx) in `crates/`: `crypto` (keys, encryption, tokens; no I/O), `db` (migrations), `pdf` (sanitizing, stamping, evidence, PAdES), `core` (services, eID, email, worker) and `server` (HTTP, REST, compatibility APIs, MCP). The web app is React, Vite and Tailwind in `web/`.

## Development

```sh
pnpm install
pnpm infra:up            # Postgres, S3, Mailpit and Gotenberg in Docker
cp .env.example .env
pnpm dev                 # server on :3000, web app on http://localhost:5173
```

| Command | |
| --- | --- |
| `pnpm lint` | Biome and Clippy |
| `pnpm typecheck` | The web app |
| `pnpm test` | Rust tests (set `TEST_DATABASE_URL`) and web tests |
| `pnpm api:types` | Regenerate the web app's typed client after changing a service |
| `docker compose --profile prod up -d --build` | The production image at http://localhost:8090 |

Email in development goes to Mailpit at http://localhost:8026. See [CLAUDE.md](CLAUDE.md) for conventions and [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute.

## Releasing

Bump `version` in `Cargo.toml`, move the **Unreleased** notes in `CHANGELOG.md` under it, merge to `main`, then publish a GitHub Release named `vX.Y.Z`. CI runs every check, builds amd64 and arm64, and pushes `ghcr.io/nielsbosma/opensign:X.Y.Z`, `:X.Y` and `:latest`. Make the package public once after the first publish (GitHub → Packages → opensign → Package settings).

## License

[MIT](LICENSE)
