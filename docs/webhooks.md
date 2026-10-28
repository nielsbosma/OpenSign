# Webhooks

Webhooks tell your systems when documents change, as signed JSON POSTs. Use them instead of polling: update your CRM when a contract is signed, start onboarding when an employment agreement is completed.

## Adding a webhook

In **Settings → Webhooks**, add the URL of your endpoint and choose the events, or all of them. You can also use `POST /api/v1/webhooks` with `{"url": "…", "events": ["document.completed"]}` (or `["*"]` for all).

OpenSign shows the webhook's **signing secret** (`whsec_…`) once. Store it with your endpoint; you need it to check signatures.

Managing webhooks needs the **webhooks.manage** permission. A test sends a `webhook.test` event, and the list shows the result of each webhook's latest delivery.

The URL must be a public `https` address without credentials in it. Private, reserved, `localhost` and `.internal` addresses are refused, when the webhook is added and again at every delivery. Redirects are not followed.

## Events

| Event | When | `data` |
| --- | --- | --- |
| `document.sent` | A draft is sent | `documentId`, `title`, `externalId` |
| `document.signed` | A party signs or approves | `documentId`, `externalId`, `partyId`, `name` |
| `document.completed` | The sealed PDF is ready | `documentId`, `title`, `externalId`, `sealedSha256` |
| `document.declined` | A party declines | `documentId`, `externalId`, `partyId`, `reason` |
| `document.canceled` | The sender cancels | `documentId`, `externalId` |
| `document.expired` | The deadline passes | `documentId` |
| `webhook.test` | You test the webhook | `webhookId`, `message` |

`externalId` is the document's **Your Reference**, so you can match events to your own records without storing OpenSign ids.

## The request

```http
POST /your/endpoint
content-type: application/json
opensign-signature: t=1793289600,v1=5d1c…e9
opensign-delivery: 019a2c4e-…

{
  "id": "019a2c4e-…",
  "event": "document.completed",
  "createdAt": "2026-10-28T15:20:00Z",
  "data": {
    "documentId": "019a2b11-…",
    "title": "Consulting agreement",
    "externalId": "crm-4711",
    "sealedSha256": "b94d27…"
  }
}
```

Webhooks carry ids and a few facts, never document contents. Fetch the document or the sealed PDF with the [REST API](api.md) when you need more.

## Checking the signature

`opensign-signature` holds a Unix timestamp `t` and `v1`, an HMAC-SHA256 over `<t>.<raw body>` with the webhook's secret, in hex. This is the same scheme Stripe uses, so existing helpers often work.

1. Take the raw request body, before any JSON parsing.
2. Compute the HMAC and compare it with `v1` in constant time.
3. Reject the request if `t` is more than a few minutes old, to stop replays.

```js
import crypto from "node:crypto";

function verify(rawBody, header, secret) {
  const parts = Object.fromEntries(header.split(",").map((p) => p.split("=")));
  if (Date.now() / 1000 - Number(parts.t) > 300) return false;
  const expected = crypto.createHmac("sha256", secret).update(`${parts.t}.${rawBody}`).digest("hex");
  return expected.length === parts.v1.length &&
    crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(parts.v1));
}
```

```python
import hmac, hashlib, time

def verify(raw_body: bytes, header: str, secret: str) -> bool:
    parts = dict(p.split("=", 1) for p in header.split(","))
    if time.time() - int(parts["t"]) > 300:
        return False
    expected = hmac.new(secret.encode(), f"{parts['t']}.".encode() + raw_body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, parts["v1"])
```

## Delivery

- Any 2xx answer within 10 seconds counts as delivered.
- Anything else is retried with backoff, starting at 30 seconds and doubling up to 6 hours, for up to 8 attempts.
- A delivery can arrive more than once. Use `opensign-delivery` (the same as the payload's `id`) to drop duplicates.
- Events are queued in the same database transaction as the change, so a change that is rolled back never fires an event, and one that is saved always does.
- Removing a webhook drops its queued deliveries.
