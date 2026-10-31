# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's **Security → Report a vulnerability** on this repository, not in a public issue. Include what an attacker can do and how to reproduce it. You will get an answer within a few days, and credit in the release notes if you want it.

## What OpenSign protects against

A signing service is only worth what its evidence is worth, so the design assumes:

- **The internet**: password guessing, stolen session cookies, cross-site requests, malicious uploads, SSRF through anything that fetches a URL, and people who hold or guess a signing link.
- **A partial compromise**: someone who reads the database or the bucket, but not `APP_SECRET`, must not be able to read documents or secrets or forge signer sessions. Write access to the database is otherwise out of scope, with one exception: the audit trail cannot be rewritten, or shortened behind its last anchor, without it showing.
- **Insiders without the right role**: every operation is checked on the server against the caller's role and the document's sharing, whichever way it arrives (web app, REST, Scrive or DocuSign APIs, MCP, AI assistant).

It does not protect against someone who holds `APP_SECRET` together with the database, or controls the host.

## Controls

**Keys and encryption**
- Every key derives from `APP_SECRET` with HKDF, one key per purpose (settings, document keys, tokens, audit chain, setup code, seal key), so one purpose's key never decrypts another's data.
- Each uploaded and sealed file gets its own random AES-256-GCM data key, wrapped with the document key-encryption key; the bucket only ever holds ciphertext, bound to its file id.
- Secret settings (API keys, certificates, client secrets), personal numbers and eID evidence are encrypted in Postgres with context-bound AES-256-GCM.
- The PAdES seal key is generated on the server and stored encrypted; it can be replaced with your own certificate.

**Accounts and sessions**
- Passwords are Unicode-normalized and hashed with Argon2id (64 MiB, 3 passes), with a minimum length. Failed sign-ins lock the sign-in for that email and address for a while, and failures from several addresses lock the account; a passkey still signs in. Every attempt is audited.
- TOTP two-factor authentication with single-use recovery codes, which an admin can require for everyone.
- Passkeys (WebAuthn, verified in-house in Rust): user verification required, origin and RP ID checked, challenges single-use and short-lived, signature counters must increase (authenticators without a counter report 0 throughout), ES256, EdDSA and RSA of 2048 bits or more. A passkey satisfies the two-factor requirement, and an account with a passkey and no authenticator needs the passkey after the password.
- Session cookies are `HttpOnly`, `SameSite=Lax`, `__Host-` prefixed and `Secure` on https, stored only as a hash in the database, with absolute and idle expiry. Cookie-authenticated writes must come from our own origin (`Origin` or `Sec-Fetch-Site`). Requests from another site are refused on `/api` even without a session, and `/api/rpc` requires a JSON content type.
- The first admin can only be created with a one-time setup code: from the server log, or `SETUP_CODE` (24+ characters) when a deployer generates one and passes it in a link's URL fragment. It stops working once the first admin exists.
- API keys and OAuth tokens are stored as hashes. MCP uses OAuth 2.1 with PKCE (S256 only), exact redirect URIs, audience-bound access tokens, and rotating refresh tokens with reuse detection that revokes the whole grant. Creating an API key or approving an MCP client asks for the password or a passkey unless the session confirmed one in the last ten minutes. Password and two-factor resets revoke API keys. Unapproved self-registered clients expire after a week, and registration can be turned off.

**Roles**
- Admin, manager, sender, auditor and viewer, enforced in the service layer. The UI only hides what the server would refuse anyway. MCP and the AI assistant act with the user's own permissions; public services are never tools. Text in documents could try to steer the assistant, so on its own it only reads, from an allowlist enforced in the service layer. Anything that changes data is shown to the signed-in user, with every value, and runs only when they confirm it. Tools that change who has access, touch webhooks or credentials, or hand out signing links are not available to it at all, and signing links are removed from what reaches the model. With document text turned off, tools that return document content are not offered.

**Signing links and signers**
- A link is the party id plus a MAC over it and a link version; it lives in the URL fragment, so it never reaches server logs or `Referer` headers, and the signing page removes it from the address bar. Rotating a link, starting in-person signing or canceling bumps the version and kills old links; a reminder resends the same link.
- Signer actions never act as a signed-in user, even if the signer's browser has an OpenSign session.
- SMS codes are 6 digits, stored as a MAC, valid 10 minutes, 5 attempts, 5 codes per hour, with an optional country allowlist and hourly and daily budgets for the install and per share link.
- eID: BankID over mutual TLS that trusts only BankID's own CA, with the party's personal number as a requirement so only that person's BankID can answer; Freja results are accepted only with a valid Freja signature over them; brokers and TIC Identity use the authorization code flow with PKCE, state, nonce and full ID token checks (signature, issuer, audience, expiry), and TIC Identity's answer counts only with a valid Swedish personal number in `nin`. When a party has a personal number, an eID that names someone else is refused and audited. Broker and TIC Identity results also need a cookie from the browser that started the flow; framed signing starts it in a window on our own origin that names the document, and the frame gets the session only with a code shown in that window after identifying (5 attempts). Signer sessions are handed out once, to a caller that also holds the link, and signing needs an identification made for signing.

**Documents**
- Uploads are checked by content, not name. PDFs are rewritten without JavaScript, actions, embedded files, XFA, launch and URI actions; Dangling references are removed and only known catalog entries stay. PDFs are parsed in a child process with no environment, a memory limit (on Linux) and a deadline, so a hostile file can only kill that process. Word files are converted by Gotenberg on a private network, which has no keys or database access.
- PDFs are served as downloads with `Content-Security-Policy: sandbox`.
- The finished document is stamped, gets evidence pages (each party's identification with personal numbers and phone numbers masked, IP, user agent and times, and the document hashes; the full values stay encrypted) and a JSON evidence attachment, then a PAdES seal over the whole file and an RFC 3161 time stamp. CI checks the result with poppler's `pdfsig` and `qpdf`.
- Retention: rules in Settings delete documents after set periods. A deletion, or replacing a file on a draft or a signer's upload, removes the file rows nothing else refers to in the same transaction, which destroys their wrapped keys so the encrypted bodies are unreadable at once, then a job removes the objects from the bucket. Files another document still uses are kept. A legal hold (its own permission, a required reason, audited) blocks the rules, manual deletion, and signers changing or dropping files. The audit trail keeps a record of every deletion.
- Long-term validation (PAdES B-LTA): after sealing, OpenSign collects every certificate the seal and time stamps depend on, fetches OCSP responses (or CRLs) for each, and adds them to the document security store, then adds a document time stamp over the whole file. Both are incremental updates, so the seal's bytes are untouched. A response is only stored after OpenSign has checked it: signed by the issuer or its delegated responder, about the right certificate, and current. Fetches go through the public HTTP client with size limits. If revocation data cannot be fetched the document is still sealed and the gap is recorded in the audit log.
- Verification checks time stamp authorities' signatures (RSA of 2048 bits or more, ECDSA P-256 and P-384, SHA-2), that their certificates are for time stamping and were valid at that time, and compares the revision each signature covers with the final file. Updates are read from the file's raw cross-reference chain and only the exact forms OpenSign writes may be added: a document security store and invisible document time stamps. Any other later change, including a new or freed object or bytes no table lists, marks the signature as not intact. A valid signature only says the file is unchanged since that key signed it: `trusted` and "sealed by this server" require one of this install's seal certificates. A time stamp counts as trusted only when its certificate is on **Trusted Timestamp Authorities** in Admin settings, which is empty by default.

**Audit trail**
- Every change is in an append-only log (database triggers refuse updates, deletes and truncation), hash-chained with a key derived from `APP_SECRET`, and its head is time-stamped by the time stamp authority every hour, so the trail up to then cannot be quietly rebuilt. `audit.verify` checks the chain, each anchor's time stamp, and a keyed copy of the latest anchor kept in the bucket, so removing entries behind the last anchor shows to someone who holds only the database. Entries newer than the last anchor are protected by the chain alone.

**The web and the network**
- Strict CSP (`script-src 'self'`, no inline script, `frame-ancestors 'none'`), HSTS on https, `nosniff`, `no-referrer`, COOP and CORP, and a tight permissions policy.
- Rate limits per address (IPv6 per /64) on sign-in, codes, signing, uploads and verification, on top of the database lockouts, with per-service body limits checked after authorization and a cap on requests in flight. The client address comes from `X-Forwarded-For` only as the **Trusted Proxies** setting allows.
- Outbound requests to anything configurable (webhooks, AI endpoints, eID brokers, time stamp authorities) resolve DNS first and refuse private, loopback, link-local and metadata addresses, and follow no redirects. The one exception is a self-hosted AI proxy (LiteLLM or another OpenAI-compatible endpoint), which an admin may point at the private network, though never at link-local or metadata addresses. Proxy environment variables are ignored.
- SMTP servers get the same address check, resolved once and connected to by that address. The connection must be TLS (implicit or STARTTLS, never a plain-text fallback) with a certificate valid for the configured host before any password is sent. The SMTP password is a settings secret: encrypted, masked in the audit log and never returned.
- The update check sends GitHub nothing but the version in the User-Agent, at most every six hours, and can be turned off. The deploy hook is stored encrypted as a secret, must be a public HTTPS address, only an admin can call it, and every call is audited.
- Webhooks are signed with a per-webhook secret in the `OpenSign-Signature` header.

**Supply chain and runtime**
- Rust with `unsafe_code = "forbid"`, rustls only (no OpenSSL), `cargo deny` for advisories and licenses, `pnpm audit` for the web app, and a pinned lockfile for both.
- The image is distroless with no shell, runs as a non-root user, and works with a read-only root file system. Release images carry provenance and an SBOM.

## Running it safely

- Generate `APP_SECRET` with `openssl rand -base64 48` (production refuses example and predictable values), store it in your platform's secret store, and back it up apart from the database. Losing it makes every document unreadable.
- Serve OpenSign over https only and set `APP_URL` to that address.
- Keep Gotenberg on a private network without internet access. Sliplane cannot enforce the no-internet part; run your own internal network if you need it.
- If your proxy is not on a private network, or clients can reach OpenSign directly from one, set **Trusted Proxies** in Admin settings → Security.
- Turn on **Require two-factor authentication** in Admin settings → Security.
- Back up Postgres and the bucket; they are useless without each other.
