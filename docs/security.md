# Security model

A signing service is only worth what its evidence is worth. OpenSign is built on the assumption that it is exposed to the internet, that parts of its infrastructure may one day be read by someone who should not, and that not every user should see everything. This page explains what it protects against and how. The repository's `SECURITY.md` has the same list in short form.

## What OpenSign protects against

- **The internet**: password guessing, stolen session cookies, cross-site requests, malicious uploads, server-side request forgery through anything that fetches a URL, and people who hold or guess a signing link.
- **A partial compromise**: someone who can read the database or the bucket, but not `APP_SECRET`, cannot read documents or secrets, forge signer sessions, or rewrite the audit trail without it showing.
- **Insiders without the right role**: every operation is checked on the server against the caller's role and the document's sharing, however it arrives.

It does not protect against someone who holds `APP_SECRET` together with the database, or who controls the host.

## Keys and encryption

- Every key derives from `APP_SECRET` with HKDF, one per purpose (settings, document keys, tokens, the audit chain, the setup code, the seal key), so one purpose's key never decrypts another's data.
- Each file gets its own random AES-256-GCM key, wrapped with the document key-encryption key. The bucket only ever holds ciphertext, bound to its file id.
- Secret settings, personal numbers and eID evidence are encrypted in Postgres, each bound to the row it belongs to, so ciphertext cannot be moved between rows.
- The seal key is generated on the server and stored encrypted.

## Accounts

- Passwords are normalized and hashed with Argon2id. Failed sign-ins lock the account for a growing period, and every attempt is recorded.
- Two-factor sign-in with an authenticator app (with single-use recovery codes) or passkeys, which an admin can require for everyone.
- Passkeys are verified in OpenSign itself: user verification required, origin and relying party checked, single-use short-lived challenges, increasing signature counters.
- Session cookies are `HttpOnly`, `SameSite=Lax`, `__Host-` prefixed and `Secure` on https, stored only as a keyed hash, with absolute and idle expiry. Cookie-authenticated writes must come from the install's own origin.
- The first admin can only be created with a one-time code from the server log.
- API keys and OAuth tokens are stored as hashes. MCP uses OAuth 2.1 with PKCE (S256 only), exact redirect URIs, audience-bound access tokens and rotating refresh tokens with reuse detection.

See [Sign-in, passkeys and two-factor](account-security.md).

## Permissions

Roles are enforced in the service layer, which every entry point uses: the web app, REST, the Scrive and DocuSign APIs, MCP and the AI assistant. The app only hides what the server would refuse anyway. MCP and the assistant act with the user's own permissions, and the assistant never gets tools that change who has access, add webhooks, change settings or hand out signing links. Secrets in settings can be cleared but never set through MCP or an API key, see [MCP](mcp.md#settings-and-secrets). See [Users and roles](users-and-roles.md).

## Signing links

- A link is the party id plus a keyed MAC over it and a link version. It sits in the URL fragment, so it never reaches server logs or `Referer` headers, and the signing page removes it from the address bar.
- Issuing a new link, canceling or finishing the document invalidates old links.
- Signer actions never use a signed-in user's session.
- SMS codes are 6 digits, stored as a MAC, valid 10 minutes, with 5 attempts and at most 5 codes per hour.
- Everything a signer can reach is rate-limited and bounded.
- Signers' AI review links are separate read-only tokens, stored as a MAC, tied to the signing link's version, capped at 100 fetches, expiring within days, and audited on every use. See [The signing page](signing-page.md#review-with-your-ai).

## Identification

- BankID runs over mutual TLS that trusts only BankID's own CA, with the party's personal number as a requirement, so only that person's BankID can answer.
- Freja results are accepted only with a valid Freja signature over them.
- Brokers and TIC Identity use the authorization code flow with PKCE, state, nonce and full ID token checks.
- When a party has a personal number, an eID naming someone else is refused and recorded.

See [Identification](identification.md).

## Documents

- Uploads are checked by content. PDFs are rewritten without JavaScript, actions, embedded files or XFA. Word files are converted by Gotenberg on a private network, which has no keys or database access.
- PDFs are served as downloads with `Content-Security-Policy: sandbox`.
- Finished documents get evidence pages, a JSON evidence attachment, a PAdES seal and an RFC 3161 time stamp, then long-term validation data and an archive time stamp. CI checks the result with `pdfsig` and `qpdf`.
- Verification rejects any change after a signature other than validation data.
- Deletion destroys a file's wrapped key at once, then removes the encrypted object.

See [Sealed PDFs and evidence](sealed-pdfs.md) and [Retention and legal hold](retention.md).

## Audit trail

Append-only by database trigger, hash-chained with a secret key, and time-stamped every hour by a time stamp authority. See [Audit trail](audit-log.md).

## Web and network

- A strict Content Security Policy (`script-src 'self'`, no inline script, `frame-ancestors 'none'`) against clickjacking, HSTS on https, `nosniff`, `no-referrer`, cross-origin isolation headers and a tight permissions policy (only the signing page may use the camera, for [webcam photos](identification.md#webcam-photo)). The web app loads nothing from other origins.
- No page can be shown in another site's frame, with one opt-in exception: when an admin turns on [embedded signing](embedding.md), the signing page names the listed sites in `frame-ancestors`. Every other page stays `frame-ancestors 'none'`, the list holds exact https origins without wildcards, and links sent by email or SMS still refuse to render in a frame.
- Rate limits per address on sign-in, codes, signing, uploads and verification, on top of the account lockouts.
- Outbound requests to anything configurable (webhooks, AI endpoints, eID brokers, time stamp authorities) resolve DNS first, refuse private, loopback, link-local and metadata addresses, and follow no redirects. The one exception is a self-hosted AI proxy, which an admin may point at the private network.
- Webhooks are signed per webhook. See [Webhooks](webhooks.md).

## Supply chain

- Rust with `unsafe_code = "forbid"`, rustls only (no OpenSSL), `cargo deny` for advisories and licenses, `pnpm audit` for the web app, and pinned lockfiles.
- The image is distroless with no shell, runs as a non-root user, and works with a read-only root file system. Release images carry provenance and an SBOM.

## Your part

See [Deployment: running it safely](deployment.md#running-it-safely): https only, `APP_SECRET` in a secret store and backed up apart from the database, Gotenberg on a private network, two-factor required, and backups of the database and bucket together.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's **Security → Report a vulnerability** on the [OpenSign repository](https://github.com/nielsbosma/OpenSign/security), not in a public issue. Include what an attacker can do and how to reproduce it. You will get an answer within a few days, and credit in the release notes if you want it.
