# Audit trail

Every change in OpenSign, every sign-in and every step a signer takes is written to an audit trail that cannot be quietly rewritten. It is the basis of the evidence in sealed documents, and it answers "who did what, and when" for anything else.

## What is recorded

- Document events: created, edited, sent, opened, identified, signed, approved, declined, canceled, expired, completed, downloaded, deleted, and legal holds placed or released.
- Account events: sign-ins (including failed ones), two-factor and passkey changes, password changes and resets.
- Administration: invitations, role changes, deactivations, API keys created, changed and revoked, webhooks added and removed, settings changes (with secret values masked), updates started.
- Integrations: API calls, MCP calls and AI assistant actions are recorded as the user they act for.

Each entry has the time, the action (such as `document.signed`), what it was about, who did it (a user, an API key, a signer or the system), their IP address and browser where it applies, and the details of the change. Personal numbers, tokens and document contents are never written to it.

## Browsing it

**Settings → Audit Trail** lists entries newest first. Filter by action, for example `document.signed`, or by what an entry was about. The **Activity** dialog on a document shows that document's own trail.

Reading the audit trail needs the **audit.read** permission, which admins and auditors have.

## Why it is tamper-evident

- **Append-only.** Database triggers refuse to update, delete or truncate audit entries, even for the database owner using OpenSign's tables normally.
- **Hash-chained.** Each entry includes a keyed hash of the entry before it, made with a key derived from `APP_SECRET`. A changed, removed or reordered entry breaks the chain from that point on.
- **Anchored in time.** Every hour, OpenSign has the head of the chain time-stamped by the time stamp authority. Someone who could rewrite the database and knew the key still could not rebuild the trail before the last anchor without the time stamps showing it.

## Verifying the chain

Choose **Verify the Chain** on the Audit Trail page (or call `GET /api/v1/audit/verify`). OpenSign recomputes every hash and checks every anchor, and reports whether the chain is intact or the first entry that was changed or removed.

## From the API

`GET /api/v1/audit` lists entries with the same filters, and `GET /api/v1/audit/verify` checks the chain. Both need an API key with the `audit.read` scope. See [REST API](api.md).
