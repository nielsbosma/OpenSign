# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's **Security → Report a vulnerability** on this repository, not in a public issue. Include what an attacker can do and how to reproduce it. You will get an answer within a few days, and credit in the release notes if you want it.

## What OpenSign protects against

A signing service is only worth what its evidence is worth, so the design assumes:

- **The internet**: password guessing, stolen session cookies, cross-site requests, malicious uploads, SSRF through anything that fetches a URL, and people who hold or guess a signing link.
- **A partial compromise**: someone who reads the database or the bucket, but not `APP_SECRET`, must not be able to read documents or secrets, forge signer sessions, or rewrite the audit trail without it showing.
- **Insiders without the right role**: every operation is checked on the server against the caller's role and the document's sharing, whichever way it arrives (web app, REST, Scrive or DocuSign APIs, MCP, AI assistant).

It does not protect against someone who holds `APP_SECRET` together with the database, or controls the host.

## Controls

**Keys and encryption**
- Every key derives from `APP_SECRET` with HKDF, one key per purpose (settings, document keys, tokens, audit chain, setup code, seal key), so one purpose's key never decrypts another's data.
- Each uploaded and sealed file gets its own random AES-256-GCM data key, wrapped with the document key-encryption key; the bucket only ever holds ciphertext, bound to its file id.
- Secret settings (API keys, certificates, client secrets), personal numbers and eID evidence are encrypted in Postgres with context-bound AES-256-GCM.
- The PAdES seal key is generated on the server and stored encrypted; it can be replaced with your own certificate.

**Accounts and sessions**
- Passwords are Unicode-normalized and hashed with Argon2id (64 MiB, 3 passes), with a minimum length. Failed sign-ins lock the account for a while; every attempt is audited.
- TOTP two-factor authentication with single-use recovery codes, which an admin can require for everyone.
- Passkeys (WebAuthn, verified in-house in Rust): user verification required, origin and RP ID checked, challenges single-use and short-lived, signature counters must increase, ES256, EdDSA and RSA of 2048 bits or more. A passkey satisfies the two-factor requirement.
- Session cookies are `HttpOnly`, `SameSite=Lax`, `__Host-` prefixed and `Secure` on https, stored only as a MAC in the database, with absolute and idle expiry. Cookie-authenticated writes must come from our own origin (`Origin` or `Sec-Fetch-Site`).
- The first admin can only be created with a one-time setup code: from the server log, or `SETUP_CODE` (24+ characters) when a deployer generates one and passes it in a link's URL fragment. It stops working once the first admin exists.
- API keys and OAuth tokens are stored as hashes. MCP uses OAuth 2.1 with PKCE (S256 only), exact redirect URIs, audience-bound access tokens, and rotating refresh tokens with reuse detection that revokes the whole grant.

**Roles**
- Admin, manager, sender, auditor and viewer, enforced in the service layer. The UI only hides what the server would refuse anyway. MCP and the AI assistant act with the user's own permissions; public services are never tools. The assistant also never gets tools that change who has access, add webhooks or hand out signing links, since text in documents could try to steer it.

**Signing links and signers**
- A link is the party id plus a MAC over it and a link version; it lives in the URL fragment, so it never reaches server logs or `Referer` headers, and the signing page removes it from the address bar. Resending or canceling bumps the version and kills old links.
- Signer actions never act as a signed-in user, even if the signer's browser has an OpenSign session.
- SMS codes are 6 digits, stored as a MAC, valid 10 minutes, 5 attempts, 5 codes per hour.
- eID: BankID over mutual TLS that trusts only BankID's own CA, with the party's personal number as a requirement so only that person's BankID can answer; Freja results are accepted only with a valid Freja signature over them; brokers and TIC Identity use the authorization code flow with PKCE, state, nonce and full ID token checks (signature, issuer, audience, expiry), and TIC Identity's answer counts only with a valid Swedish personal number in `nin`. When a party has a personal number, an eID that names someone else is refused and audited. Signer sessions are handed out once, to a caller that also holds the link.

**Documents**
- Uploads are checked by content, not name. PDFs are rewritten without JavaScript, actions, embedded files, XFA, launch and URI actions; Word files are converted by Gotenberg on a private network, which has no keys or database access.
- PDFs are served as downloads with `Content-Security-Policy: sandbox`.
- The finished document is stamped, gets evidence pages (each party's identification, IP, user agent and times, and the document hashes) and a JSON evidence attachment, then a PAdES seal over the whole file and an RFC 3161 time stamp. CI checks the result with poppler's `pdfsig` and `qpdf`.
- Retention: rules in Settings delete documents after set periods. A deletion removes the document's file rows in the same transaction, which destroys their wrapped keys so the encrypted bodies are unreadable at once, then a job removes the objects from the bucket. Files another document still uses are kept. A legal hold (its own permission, a required reason, audited) blocks both the rules and manual deletion. The audit trail keeps a record of every deletion.
- Long-term validation (PAdES B-LTA): after sealing, OpenSign collects every certificate the seal and time stamps depend on, fetches OCSP responses (or CRLs) for each, and adds them to the document security store, then adds a document time stamp over the whole file. Both are incremental updates, so the seal's bytes are untouched. A response is only stored after OpenSign has checked it: signed by the issuer or its delegated responder, about the right certificate, and current. Fetches go through the public HTTP client with size limits. If revocation data cannot be fetched the document is still sealed and the gap is recorded in the audit log.
- Verification checks time stamp authorities' signatures (RSA of 2048 bits or more, ECDSA P-256 and P-384, SHA-2), and compares the revision each signature covers with the final file: only a document security store and invisible document time stamps may be added. Any other later change, including a new annotation or a changed catalog entry, marks the signature as not intact.

**Audit trail**
- Every change is in an append-only log (database triggers refuse updates, deletes and truncation), hash-chained with a key derived from `APP_SECRET`, and its head is time-stamped by the time stamp authority every hour, so the trail up to then cannot be quietly rebuilt. `audit.verify` checks the chain and the anchors.

**The web and the network**
- Strict CSP (`script-src 'self'`, no inline script, `frame-ancestors 'none'`), HSTS on https, `nosniff`, `no-referrer`, COOP and CORP, and a tight permissions policy.
- Rate limits per address on sign-in, codes, signing, uploads and verification, on top of the database lockouts.
- Outbound requests to anything configurable (webhooks, AI endpoints, eID brokers, time stamp authorities) resolve DNS first and refuse private, loopback, link-local and metadata addresses, and follow no redirects. The one exception is a self-hosted AI proxy (LiteLLM or another OpenAI-compatible endpoint), which an admin may point at the private network.
- SMTP servers get the same address check, resolved once and connected to by that address. The connection must be TLS (implicit or STARTTLS, never a plain-text fallback) with a certificate valid for the configured host before any password is sent. The SMTP password is a settings secret: encrypted, masked in the audit log and never returned.
- The update check sends GitHub nothing but the version in the User-Agent, at most every six hours, and can be turned off. The deploy hook is stored encrypted as a secret, must be a public HTTPS address, only an admin can call it, and every call is audited.
- Webhooks are signed with a per-webhook secret in the `OpenSign-Signature` header.

**Supply chain and runtime**
- Rust with `unsafe_code = "forbid"`, rustls only (no OpenSSL), `cargo deny` for advisories and licenses, `pnpm audit` for the web app, and a pinned lockfile for both.
- The image is distroless with no shell, runs as a non-root user, and works with a read-only root file system. Release images carry provenance and an SBOM.

## Running it safely

- Generate `APP_SECRET` with `openssl rand -base64 48`, store it in your platform's secret store, and back it up apart from the database. Losing it makes every document unreadable.
- Serve OpenSign over https only and set `APP_URL` to that address.
- Keep Gotenberg on a private network without internet access.
- Turn on **Require two-factor authentication** in Admin settings → Security.
- Back up Postgres and the bucket; they are useless without each other.
