# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/) and versions follow [Semantic Versioning](https://semver.org/). Database migrations always run forward.

## [Unreleased]

### Security

A security review of 0.1.0 led to this round of hardening. Update as soon as you can.

- PDFs are parsed in a separate process with memory and time limits, and every parser has limits on what it reads. Spreadsheet import and formula evaluation are bounded as well.
- Signature verification reads the raw update chain of the file, accounts for every byte after a signature, and reports `trusted` separately from intact. Public verification has its own worker limit and answers 503 when busy. The PDF sanitizer is stricter.
- The audit chain only accepts keyed entries. `audit.verify` checks each anchor's time stamp and a keyed copy of the latest anchor in the bucket.
- The assistant only reads on its own. Anything that changes data becomes a proposal the signed-in user confirms, and turning document text off now covers every tool.
- Sign-in: passkey accounts need the passkey after the password, the two-factor requirement also covers file routes, lockouts are per email and address, and password hashing is capped.
- Broker and TIC Identity results are bound to the browser that started them. From a framed signing page, the person also enters a code shown in the identification window. Sealed evidence masks personal numbers and phone numbers.
- Files replaced on a draft or by a signer are destroyed. Legal hold also refuses edits and canceling.

### Fixed

- The assistant no longer turns the whole page blank when a message arrives, in browsers where `scrollIntoView` returns a promise.

### Changed

- Creating an API key or approving an MCP client can ask for your password or passkey again. Password and two-factor resets revoke API keys and tell the owner.
- `/api/rpc` requires `Content-Type: application/json` when there is a body, and bodies are limited to 2 MB except on upload-like services.
- Production refuses example and predictable `APP_SECRET` values. If yours is one, plan a migration before updating: changing it makes existing data unreadable.
- Bulk send reads CSV and `.xlsx` only; `.xls` and `.ods` are refused.
- Formulas are limited to 64 operators and chains of 32 fields.
- Sender-defined text patterns are checked by the server when signing, not while typing.
- Deleting a canceled document that someone already signed needs permission to manage all documents.
- Proxy environment variables are ignored for outbound requests.

### Added

- `documents.party_identity` shows how a party identified with the number in full, for anyone who may see all documents, and audits each use.
- Admin settings: **Trusted Timestamp Authorities**, **Trusted Proxies**, a switch for dynamic MCP client registration, an SMS country allowlist and SMS budgets.
- A per-user hourly limit on assistant model calls.

## [0.1.0] - 2026-10-29

The first release: electronic signatures you run yourself, in one Docker image with Postgres and an S3 bucket.

### Running It

- One distroless, non-root image (web app, API, MCP and job worker) at `ghcr.io/nielsbosma/opensign`, configured with Postgres, an S3 bucket, `APP_SECRET` and `APP_URL`. Every other setting lives in Admin settings and can be pinned with `OPENSIGN_<GROUP>_<FIELD>`. Migrations run on every start.
- Deploy to Sliplane with one button (the Sliplane Deployer and `sliplane.yaml`), with `deploy/sliplane.sh` from a terminal, or with `deploy/release.ps1`, which releases and deploys step by step. The Deployer generates `APP_SECRET` and a `SETUP_CODE` and its finish page opens setup with the code already filled in.
- Setup behind a one-time code: the first admin, then email through Resend or SMTP (STARTTLS or TLS, certificates verified) with a test message before anything can be sent.
- Settings → System shows the running version against the latest GitHub release, migrations and the job queue, and tells admins about updates. **Update Now** calls a Sliplane deploy hook or opens the Deployer's update link with the new version; CI can call the deploy hook after each release.
- Documentation at `/docs` on every install, public and searchable, linked from the sidebar, settings pages, the document list and the party dialog.

### Documents and Templates

- Word and PDF uploads (Word converted by Gotenberg on a private network), PDF sanitizing, templates, sharing, reminders and expiry. Deleting a document deletes its files too, unless another document still uses them.
- Field placement by drag and drop: signatures, initials, text, dates, checkboxes, party details, number, dropdown, multi-select, radio groups, phone, date picker, image and in-place file upload. Text fields take a pattern, a length and your own error text; numbers and dates take limits and formats such as `1 234,00 kr` or DD/MM/YYYY. Fields can have defaults and be read-only. Every check runs on the signing page and again on the server.
- Conditional fields and formulas such as `round({Hours} * {Rate} * 1.25, 2)`, parsed by hand and never evaluated as code; the server recomputes formulas and leaves hidden fields off the sealed PDF.
- Text tags in PDF and Word files (`{{Name;role=Tenant;type=signature}}`, DocuSeal's syntax), fillable PDF forms imported as fields with their values, and **Detect Fields**, which suggests fields from lines, underscores and boxes, optionally with the AI model.
- Template versions: every use of a changed template saves a version, so each document names the exact version it came from. History, compare, restore with a note, and replace a file while keeping its fields.
- Folders (nested up to three levels) and an **Archived** tab for documents and templates. Folders never grant access.
- Export filled-in data as Excel, CSV or JSON from the document list, a template, a bulk send or a single document. Personal numbers and file contents are never included, CSV cells are guarded against formula injection, and every export is audited.
- Bulk send: a template to every row of a CSV, Excel or OpenDocument file (up to 1,000 rows), with automatic or AI column matching, row checks before sending, background sending and a progress page. Row data is stored encrypted and dropped once each row is sent.
- Retention rules delete signed, ended and draft documents after set periods (keys destroyed at once, bucket objects removed by a job). Legal holds, with a required reason, keep a document from every rule and from manual deletion.
- Company logo and brand color on signing and verification pages, in emails and on the evidence page of sealed PDFs. PNG or JPEG only, re-encoded on upload and stored encrypted.

### Signing

- Signers, approvers and viewers in order or in parallel; delivery by email, SMS (46elks or Twilio) or link; drawn or typed signatures; decline with a reason.
- Identification by SMS code, Swedish BankID (own agreement or through TIC Identity, no bank agreement needed), Freja eID or an OpenID Connect eID broker such as Idura or Signicat, with personal-number matching. Certificate files upload as they come from the bank, and BankID test mode works with no setup.
- Webcam photo identification: the signer takes a live photo before the Sign button unlocks. It is stored encrypted and sealed into the evidence with its SHA-256. It is not a liveness check.
- Signer attachments (PDF, JPEG or PNG, required or optional), sanitized or re-encoded, stored encrypted and embedded in the sealed PDF.
- In-person signing on the sender's device, with a hand-over screen and the host recorded in the evidence.
- **Sign Myself** for documents only you sign, saved signatures and initials on your account, and public share links with QR codes for documents anyone can fill in and sign after confirming their email address.
- **Preview** shows the real signing page as a chosen party will see it, with nothing audited or sent.
- **Review with Your AI**: a signer can give their own AI assistant a separate, read-only, expiring link to the document. The signing link never goes into an AI.
- Embedded signing, off by default: an `<opensign-signing>` element for the sites an admin lists, with React and Vue wrappers in `packages/`.
- Eleven languages for signers and the whole app: English, Chinese (Simplified), Hindi, Spanish, French, Arabic, Bengali, Portuguese, Russian, Urdu and Swedish, with right-to-left layout for Arabic and Urdu. Each party and each user can have their own.

### Sealed PDFs and Evidence

- Stamped values, evidence pages and a JSON evidence attachment, a PAdES seal over the whole file and an RFC 3161 time stamp.
- Long-term validation: sealed PDFs are PAdES B-LTA, with certificates, OCSP or CRL answers and an archive time stamp in the file, so they validate offline after the certificates expire.
- A public `/verify` page and API that check a sealed PDF without storing it, and reject any later change other than validation data.
- A tamper-evident audit trail: append-only, hash-chained and time-stamped hourly.

### Accounts and Security

- Roles: admin, manager, sender, auditor and viewer. Invites, password reset, TOTP two-factor with recovery codes, and passkeys (Face ID, Touch ID, Windows Hello, a phone or a security key) that count as two-factor.
- Files are encrypted per file with AES-256-GCM before they reach the bucket; keys derive from `APP_SECRET` per purpose. Personal numbers and secrets are stored encrypted, tokens only as a MAC or hash.
- A strict Content Security Policy with no inline script and no external origins, and outbound requests that refuse private addresses and redirects.
- See `SECURITY.md` for the full model.

### APIs and Integrations

- REST with OpenAPI, idempotency keys and API keys with permissions and an IP allowlist, editable after creation (narrowing only, through an API key).
- Drop-in compatible endpoints for Scrive Document API v2 (`/scrive/api/v2`) and DocuSign eSignature v2.1 (`/docusign/restapi/v2.1`).
- API Explorer: every endpoint with its schemas, live requests as yourself or with an API key, and the call as curl, JavaScript or Python.
- Signed webhooks with `document.created`, `document.viewed`, `document.completed` and more, retries, and a 30-day delivery log under Settings → Webhooks with **Resend**.
- An MCP server over OAuth 2.1 (dynamic registration, PKCE, rotating refresh tokens) that can do everything a role allows except credentials: passwords, two-factor, passkeys, creating API keys and the seal key stay in the app.
- Automations through Composio, off by default: connect apps such as Google Drive, Slack, HubSpot or Google Sheets and run a tool when a document event happens.
- An optional AI assistant, pre-send document review and a model picked from the provider's own list, with a monthly token budget. Document text is treated as data, never as instructions.

### Known Limits

- Text stamped into PDFs uses WinAnsi, which covers Latin-1 including the Nordic letters: other scripts, such as names in Chinese or Arabic, print as `?`.
- The evidence page of sealed PDFs is in English only, and server error messages are not translated yet.
- The ten non-English languages have not been reviewed by native speakers.
- BankID through TIC Identity, Freja eID, eID brokers, Composio and the AI providers are built to their documentation but not yet tested against the live services.
