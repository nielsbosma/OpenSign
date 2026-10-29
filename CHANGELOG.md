# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/) and versions follow [Semantic Versioning](https://semver.org/). Database migrations always run forward.

## [Unreleased]

### Changed

- The database migrations are squashed into one, `0001_initial.sql`, before the first release. A development database made with the old migrations must be recreated.

### Added

- MCP covers everything a role allows except credentials: `settings_update` (secrets can be cleared, never set, and a saved secret cannot be pointed at another server), `documents_review`, `ai_models`, `ai_test`, `users_change_email` and `system_update` are now tools, plus `docs_search` and `docs_read` over the documentation and one-time upload and short-lived download links for big files (`documents_upload_url`, `documents_download_url`). Every service states whether it is a tool and why not, and carries its own destructive and open-world hints.
- Company logo in Settings → Organisation, shown in the app, on signing and verification pages, in emails and on the evidence page of sealed PDFs. PNG or JPEG only, re-encoded on upload and stored encrypted; SVG is refused. The brand color picker is a round swatch, and the color now tints the OpenSign mark when there is no logo.
- Documentation at `/docs` on every install, public and searchable, rendered from the Markdown in `docs/`. The app links to it from the sidebar, settings pages, the document list and the party dialog.
- SMTP as an alternative to Resend, in setup and Admin settings: host, port, STARTTLS or TLS (certificates verified, never plain text), optional sign-in with the password stored encrypted, and the same test email before anything is sent.
- REST routes for bulk send (`/bulk-sends`, with `/bulk-sends/preview` for column mapping), legal hold (`PUT /documents/{id}/legal-hold`), the retention preview and system version and status, all in the OpenAPI document.
- API Explorer: browse every REST endpoint with its schemas, send live requests as yourself or with an API key, and copy the call as curl, JavaScript or Python.

- Settings → System: the running version against the latest GitHub release, migrations and the job queue; an update notice for admins; one-click update through a Sliplane deploy hook. Migrations run on every start.

- One distroless Docker image (web app, API, MCP and job worker), configured with Postgres, an S3 bucket, `APP_SECRET` and `APP_URL`, and a Sliplane deploy script.
- Setup behind a one-time code: first admin, then Resend email with a test message before anything can be sent.
- Accounts and roles: admin, manager, sender, auditor and viewer; TOTP two-factor with recovery codes; invites and password reset; API keys.
- Swedish BankID through TIC Identity (OpenID Connect) for installs without their own BankID agreement, with the same personal-number matching.
- Documents: Word and PDF uploads (Word converted by Gotenberg), PDF sanitizing, drag-and-drop field placement for signatures, initials, text, dates, checkboxes and party details, templates, sharing, reminders and expiry.
- Signing: signers, approvers and viewers in order or in parallel; email, SMS or link delivery; SMS codes, Swedish BankID, Freja eID and OpenID Connect eID brokers, with personal-number matching; drawn or typed signatures; decline with a reason.
- Swedish for signers: emails, SMS, the signing page and BankID messages follow the document's language, which defaults to the organisation's.
- Signer attachments: ask each party for files (PDF, JPEG or PNG, required or optional). Uploads are sanitized or re-encoded, stored encrypted, listed with their SHA-256 in the evidence and embedded in the sealed PDF.
- In-person signing: a party can sign on the sender's device. Starting it retires earlier links, shows a hand-over screen, and records who hosted it in the evidence. Scrive's `pad` delivery maps to it.
- Retention rules in Settings: delete signed, ended and draft documents after set periods, with their files (keys destroyed at once, bucket objects removed by a job). Legal holds, with a required reason, keep a document from every rule and from manual deletion. The document page shows how long it is kept.
- Deleting a document now deletes its files too, unless another document still uses them.
- Long-term validation: sealed PDFs are PAdES B-LTA, with the certificates and OCSP or CRL answers stored in the file and an archive time stamp over everything, so they validate offline after the certificates expire. The verifier checks time stamp signatures and rejects any later change other than validation data.
- Passkeys: sign in with Face ID, Touch ID, Windows Hello, a phone or a security key. Passkeys require user verification, so they count as two-factor; adding one asks for the password, and each sign-in challenge works once.
- Bulk send: a template to every row of a CSV, Excel or OpenDocument file (up to 1,000 rows), with automatic column matching, optional AI matching, row checks before sending, background sending and a progress page. Row data is stored encrypted and dropped once each row is sent.
- eID setup: certificate files upload as they come from the bank (`.p12`/`.pfx` with their password), BankID's server roots are built in, and BankID test mode works with no setup.
- Sealed PDFs: stamped values, evidence pages and a JSON evidence attachment, a PAdES seal over the whole file and an RFC 3161 time stamp; a verify page and API.
- APIs: REST with OpenAPI, Scrive v2 (`/scrive/api/v2`) and DocuSign eSignature v2.1 (`/docusign/restapi/v2.1`) compatible endpoints, signed webhooks, idempotency keys.
- MCP server with OAuth 2.1 (dynamic registration, PKCE, rotating refresh tokens).
- Optional AI assistant and pre-send document review with a monthly token budget.
- Tamper-evident audit trail, hash-chained and time-stamped hourly.
