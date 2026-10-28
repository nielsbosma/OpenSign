# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/) and versions follow [Semantic Versioning](https://semver.org/). Database migrations always run forward.

## [Unreleased]

### Added

- One distroless Docker image (web app, API, MCP and job worker), configured with Postgres, an S3 bucket, `APP_SECRET` and `APP_URL`, and a Sliplane deploy script.
- Setup behind a one-time code: first admin, then Resend email with a test message before anything can be sent.
- Accounts and roles: admin, manager, sender, auditor and viewer; TOTP two-factor with recovery codes; invites and password reset; API keys.
- Swedish BankID through TIC Identity (OpenID Connect) for installs without their own BankID agreement, with the same personal-number matching.
- Documents: Word and PDF uploads (Word converted by Gotenberg), PDF sanitizing, field placement for signatures, initials, text, dates, checkboxes and party details, templates, sharing, reminders and expiry.
- Signing: signers, approvers and viewers in order or in parallel; email, SMS or link delivery; SMS codes, Swedish BankID, Freja eID and OpenID Connect eID brokers, with personal-number matching; drawn or typed signatures; decline with a reason.
- Swedish for signers: emails, SMS, the signing page and BankID messages follow the document's language, which defaults to the organisation's.
- Signer attachments: ask each party for files (PDF, JPEG or PNG, required or optional). Uploads are sanitized or re-encoded, stored encrypted, listed with their SHA-256 in the evidence and embedded in the sealed PDF.
- In-person signing: a party can sign on the sender's device. Starting it retires earlier links, shows a hand-over screen, and records who hosted it in the evidence. Scrive's `pad` delivery maps to it.
- eID setup: certificate files upload as they come from the bank (`.p12`/`.pfx` with their password), BankID's server roots are built in, and BankID test mode works with no setup.
- Sealed PDFs: stamped values, evidence pages and a JSON evidence attachment, a PAdES seal over the whole file and an RFC 3161 time stamp; a verify page and API.
- APIs: REST with OpenAPI, Scrive v2 and DocuSign eSignature v2.1 compatible endpoints, signed webhooks, idempotency keys.
- MCP server with OAuth 2.1 (dynamic registration, PKCE, rotating refresh tokens).
- Optional AI assistant and pre-send document review with a monthly token budget.
- Tamper-evident audit trail, hash-chained and time-stamped hourly.
