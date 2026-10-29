# Working on OpenSign

Instructions for coding agents and people. Read the README and SECURITY.md first: security is the product here, so a change that weakens a control in SECURITY.md is a bug, however convenient.

## Golden rules

- **All logic lives in a service** in `crates/core/src/services`: a struct implementing `Service` with `Input`, `Output` (both `JsonSchema`) and a `META`. Register it in `services::registry()`. The RPC endpoint, MCP and the AI assistant pick it up from there; REST routes are a table in `crates/server/src/rest.rs`, and the Scrive and DocuSign APIs in `crates/server/src/compat` call the same services. Handlers never touch the database themselves.
- **Every service declares `Access`**: a `Permission`, `Authenticated` or `Public`. Add permissions in `permissions.rs` and to the roles that need them. The UI only hides things; the service decides.
- **Every change calls `audit(ctx, conn, Entry::new(action, entity_type, id)...)`.** Actions are `<area>.<verb_snake>`. Pass `secret_keys` for anything secret.
- **Descriptions are written for a model**: they become MCP and assistant tool descriptions. Full sentences that say what it does and when to use it.
- Mutations run in a transaction. `without_transaction()` only when a failure must still persist writes (attempt counters) or the service calls out to the network (eID, SMS). `not_a_tool()` for anything with credentials or the caller's own session; public services are never tools.
- Signer-facing services are named `signing.*`, take the link token, and resolve the party with `signing::resolve`. They never use the signed-in user.
- After changing a service's input or output, run `pnpm api:types` and commit `web/src/lib/api-types.gen.ts`.
- A change people can see updates its page in `docs/` (served at `/docs`). A new page also goes in the `NAV` list in `web/src/pages/docs/content.ts`.

## Security habits

- Never log or return secrets, tokens, personal numbers or document contents. Errors shown to users come from `AppError` messages; causes stay in the log.
- Encrypt new secrets with `ctx.keys().encrypt_str(KeyPurpose::..., context, ...)` and a context that names the row, so ciphertext cannot be moved between rows.
- Outbound HTTP to anything configurable goes through `ctx.app.http.public` (blocks private addresses, no redirects). `internal` is only for the converter and development mail.
- Compare secrets with `constant_time_eq`. Store tokens as a MAC or hash, never in the clear.
- Anything a signer can reach is rate-limited and bounded (attempts, expiry, size).
- New dependencies: prefer pure Rust, rustls only (no OpenSSL; `cargo deny` refuses it).

## Settings and configuration

- Environment variables are only `DATABASE_URL`, `S3_*`, `APP_SECRET`, `APP_URL`, `CONVERTER_URL`, `PORT`, `ROLE`, `APP_ENV`. Runtime configuration belongs in Admin settings.
- A new setting: a field on its group in `crates/core/src/settings.rs` (list secrets in `SECRETS`), validation in `admin::validate`, and a line in the `GROUPS` table in `web/src/pages/settings/AdminSettingsPage.tsx`. It is stored encrypted if secret, cached, audited, and can be pinned with `OPENSIGN_<GROUP>_<FIELD>`.

## Data

- Migrations are numbered SQL files in `crates/db/migrations`. Add a new one; never edit one that has been released. Use `check` constraints for enums and `timestamptz` for times.
- The audit log and anchors are append-only by trigger. Do not work around it.

## UI

- React, Tailwind and shadcn/ui components in `web/src/components/ui`. Data: `useApi(name, input)` and `useApiMutation(name, { invalidates })`; errors toast unless `silent`.
- The CSP allows no inline script and no external origins: no CDNs, no inline `<script>`, no `eval`.
- **Labels use AP-style Title Case** (headings, buttons, tabs, field labels, menu items): "Send for Signing", "Add a Party". Descriptions, hints, placeholders and toasts are sentence case.
- Neutral greys, one indigo accent, subtle borders, light and dark.
- **Every text a person reads is translated** into en, zh, hi, es, fr, ar, bn, pt, ru, ur and sv. Write it in English with `t('Text {name}', { name })` from `@/lib/i18n`; use `msg()` where a text is defined in a table and `t()` where it is shown, `tx()` for links or code inside a sentence. Add the translation to every `web/src/locales/*.json` (the locales test fails otherwise). Use logical classes (`ms-`, `pe-`, `start-`, `text-start`) so Arabic and Urdu lay out right to left. The signing page has its own texts in `web/src/pages/sign`, emails in `crates/core/src/i18n.rs`.

## Checks before you finish

```sh
pnpm lint && pnpm typecheck && pnpm test   # with pnpm infra:up and TEST_DATABASE_URL set
```

For signing or sealing changes, also check the sealed PDF: `SEALED_PDF_OUT=sealed.pdf cargo test -p opensign-core --test signing_flow`, then `pdfsig sealed.pdf` and `qpdf --check sealed.pdf` (CI does this).

## Style

- Comments only where they say why, never what. Keep them rare.
- Follow the nearest existing example; consistency beats cleverness.
- Never commit secrets. `.env` and `.env.local` are gitignored.
