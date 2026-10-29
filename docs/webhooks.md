# Webhooks

Webhooks tell your systems when documents change, as signed JSON POSTs. Use them instead of polling: update your CRM when a contract is signed, start onboarding when an employment agreement is completed, or chase a declined offer. To send documents to apps such as Google Drive, Slack or HubSpot without an endpoint of your own, use [automations](automations.md).

## Adding a webhook

1. Open **Settings → Webhooks** and choose **New Webhook**.
2. Enter the URL of your endpoint and pick the events you want, or all of them.
3. Copy the **signing secret** (`whsec_…`). It is shown only once. Store it with your endpoint; you need it to [check signatures](#checking-the-signature).
4. Choose **Test** to send a `webhook.test` event and see that your endpoint answers.

The list shows each webhook's events, the result of its latest delivery (the status code your endpoint answered, or the error, such as a timeout) and how many deliveries failed. **Deliveries** opens the [delivery log](#delivery-log). **Delete** stops deliveries at once and drops any that are still queued.

Managing webhooks needs the **webhooks.manage** permission (admins and managers). Every change is in the [audit trail](audit-log.md).

### Rules for the URL

- It must be a public `https` address, without a user name or password in it.
- Private, reserved, `localhost` and `.internal` addresses are refused, both when the webhook is added and again at every delivery.
- Redirects are not followed. Point the webhook at the final address.

### With the API

Webhooks can also be managed with the [REST API](api.md), with an API key that has the `webhooks.manage` scope.

| Request | |
| --- | --- |
| `GET /api/v1/webhooks` | List webhooks with their latest delivery result and the number of failed deliveries. |
| `POST /api/v1/webhooks` | Add one: `{"url": "https://…", "events": ["document.completed"]}`, or `["*"]` for every event. The answer includes the secret, once. |
| `POST /api/v1/webhooks/{id}/test` | Send a `webhook.test` event. |
| `DELETE /api/v1/webhooks/{id}` | Remove one. |
| `GET /api/v1/webhooks/{webhookId}/deliveries` | The [delivery log](#delivery-log), newest first. Filter with `status` (`pending`, `delivered` or `failed`) and `event`; page with `before` (the last id you got) and `limit` (up to 200). |
| `GET /api/v1/webhook-deliveries/{id}` | One delivery with its payload and every attempt. |
| `POST /api/v1/webhook-deliveries/{id}/resend` | Send a delivery again. Answers with the new delivery's `id`. |
| `POST /api/v1/webhooks/{webhookId}/resend-failed` | Resend every delivery that was given up and not resent yet, up to 500. Answers with `count`. |

## Events

| Event | When | `data` |
| --- | --- | --- |
| `document.created` | A document is created, from scratch, from a template or by a bulk send. Templates fire nothing. | `documentId`, `title`, `externalId`, `templateId` |
| `document.sent` | A draft is sent for signing | `documentId`, `title`, `externalId` |
| `document.viewed` | A party opens their signing link for the first time (after identifying, if that is required to view) | `documentId`, `externalId`, `partyId`, `name` |
| `document.signed` | A party signs or approves | `documentId`, `externalId`, `partyId`, `name` |
| `document.completed` | Everyone has signed and the sealed PDF is ready | `documentId`, `title`, `externalId`, `sealedSha256`, `valuesUrl` |
| `document.declined` | A party declines | `documentId`, `externalId`, `partyId`, `reason` |
| `document.canceled` | The sender or a manager cancels | `documentId`, `externalId` |
| `document.expired` | The deadline passes before everyone signed | `documentId` |
| `webhook.test` | You choose **Test** | `webhookId`, `message` |

`externalId` is the document's **Your Reference**, or `null` when it has none, so you can match events to your own records without storing OpenSign ids.

A document that is signed by three parties fires `document.created` and `document.sent`, then `document.viewed` and `document.signed` once per party, then `document.completed` once the seal is done, usually seconds after the last signature.

Webhooks carry ids and a few facts, never document contents, field values or personal numbers. Fetch the document, its parties or the sealed PDF with the [REST API](api.md) when you need more. `document.completed` includes `valuesUrl`, where an API key reads everything the parties filled in. See [Exporting filled-in data](data-export.md).

## The request

```http
POST /your/endpoint HTTP/1.1
content-type: application/json
opensign-signature: t=1793289600,v1=5d1c…e9
opensign-delivery: 019a2c4e-7b1f-7c3a-9e51-3f0d2a8b6c11

{
  "id": "019a2c4e-7b1f-7c3a-9e51-3f0d2a8b6c11",
  "event": "document.completed",
  "createdAt": "2026-10-28T15:20:00.412Z",
  "data": {
    "documentId": "019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28",
    "title": "Consulting agreement",
    "externalId": "crm-4711",
    "sealedSha256": "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
    "valuesUrl": "https://sign.example.com/api/v1/documents/019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28/values"
  }
}
```

| Part | |
| --- | --- |
| `id` | The delivery id. The same as the `opensign-delivery` header, and the same on every retry of this delivery. A resend gets a new one. |
| `event` | The event name. |
| `createdAt` | When the event happened, in UTC. |
| `data` | The event's facts, below. |
| `opensign-signature` | The signature. See [Checking the signature](#checking-the-signature). |

### Payload examples

`document.created`:

```json
{
  "id": "019a2c3f-…",
  "event": "document.created",
  "createdAt": "2026-10-28T09:01:40.311Z",
  "data": { "documentId": "019a2b11-…", "title": "Consulting agreement", "externalId": "crm-4711", "templateId": "019a1f02-…" }
}
```

`templateId` is `null` when the document was not made from a template.

`document.sent`:

```json
{
  "id": "019a2c40-…",
  "event": "document.sent",
  "createdAt": "2026-10-28T09:02:11.087Z",
  "data": { "documentId": "019a2b11-…", "title": "Consulting agreement", "externalId": "crm-4711" }
}
```

`document.viewed`:

```json
{
  "id": "019a2c43-…",
  "event": "document.viewed",
  "createdAt": "2026-10-28T13:40:02.774Z",
  "data": { "documentId": "019a2b11-…", "externalId": "crm-4711", "partyId": "019a2b12-…", "name": "Anna Svensson" }
}
```

`document.signed`:

```json
{
  "id": "019a2c45-…",
  "event": "document.signed",
  "createdAt": "2026-10-28T13:47:53.530Z",
  "data": { "documentId": "019a2b11-…", "externalId": "crm-4711", "partyId": "019a2b12-…", "name": "Anna Svensson" }
}
```

`document.declined`:

```json
{
  "id": "019a2c47-…",
  "event": "document.declined",
  "createdAt": "2026-10-28T14:05:30.002Z",
  "data": { "documentId": "019a2b11-…", "externalId": "crm-4711", "partyId": "019a2b12-…", "reason": "The notice period should be one month." }
}
```

`reason` is `null` when the party gave none.

`document.canceled`:

```json
{
  "id": "019a2c48-…",
  "event": "document.canceled",
  "createdAt": "2026-10-28T14:30:00.910Z",
  "data": { "documentId": "019a2b11-…", "externalId": "crm-4711" }
}
```

`document.expired`:

```json
{
  "id": "019a2c49-…",
  "event": "document.expired",
  "createdAt": "2026-11-28T00:05:00.000Z",
  "data": { "documentId": "019a2b11-…" }
}
```

`webhook.test`:

```json
{
  "id": "019a2c4a-…",
  "event": "webhook.test",
  "createdAt": "2026-10-28T08:00:00.000Z",
  "data": { "webhookId": "019a2c00-…", "message": "Test event from OpenSign" }
}
```

## Checking the signature

Anyone can send a POST to your endpoint, so check every request's signature before you trust it.

`opensign-signature` holds `t`, a Unix timestamp in seconds, and `v1`, an HMAC-SHA256 in hex over the string `<t>.<raw body>`, keyed with the webhook's secret (the whole `whsec_…` string). This is the same scheme Stripe uses, so existing helpers often work.

1. Take the raw request body, exactly as received, before any JSON parsing.
2. Compute the HMAC over `t`, a dot, and the body, and compare it with `v1` in constant time.
3. Reject the request if `t` is more than a few minutes old, to stop replays.

Node.js:

```js
import crypto from "node:crypto";

function verify(rawBody, header, secret) {
  const parts = Object.fromEntries(header.split(",").map((p) => p.split("=")));
  if (Math.abs(Date.now() / 1000 - Number(parts.t)) > 300) return false;
  const expected = crypto.createHmac("sha256", secret).update(`${parts.t}.${rawBody}`).digest("hex");
  return expected.length === parts.v1.length &&
    crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(parts.v1));
}
```

With Express, read the body with `express.raw({ type: "application/json" })` so it is not parsed first.

Python:

```python
import hmac, hashlib, time

def verify(raw_body: bytes, header: str, secret: str) -> bool:
    parts = dict(p.split("=", 1) for p in header.split(","))
    if abs(time.time() - int(parts["t"])) > 300:
        return False
    expected = hmac.new(secret.encode(), f"{parts['t']}.".encode() + raw_body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, parts["v1"])
```

C#:

```csharp
using System.Security.Cryptography;
using System.Text;

static bool Verify(byte[] rawBody, string header, string secret)
{
    var parts = header.Split(',').Select(p => p.Split('=', 2)).ToDictionary(p => p[0], p => p[1]);
    var t = long.Parse(parts["t"]);
    if (Math.Abs(DateTimeOffset.UtcNow.ToUnixTimeSeconds() - t) > 300) return false;
    var signed = Encoding.UTF8.GetBytes($"{t}.").Concat(rawBody).ToArray();
    var expected = Convert.ToHexString(HMACSHA256.HashData(Encoding.UTF8.GetBytes(secret), signed)).ToLowerInvariant();
    return CryptographicOperations.FixedTimeEquals(Encoding.ASCII.GetBytes(expected), Encoding.ASCII.GetBytes(parts["v1"]));
}
```

If you lose the secret, delete the webhook and add it again to get a new one.

## Delivery and retries

- Any `2xx` answer within 10 seconds counts as delivered. The body of your answer is ignored.
- Anything else, including a timeout, a refused connection or a `3xx`, `4xx` or `5xx` answer, is retried.
- A delivery is tried up to 8 times. After the last failure it is given up. It shows as **Failed** in the [delivery log](#delivery-log), where you can resend it.

| Attempt | About when |
| --- | --- |
| 1 | Right away |
| 2 | 30 seconds after the first failure |
| 3 | 1 minute later |
| 4 | 2 minutes later |
| 5 | 4 minutes later |
| 6 | 8 minutes later |
| 7 | 16 minutes later |
| 8 | 32 minutes later, about an hour after the event |

Each retry is signed again with a fresh `t`, so a delivery that waited is not mistaken for a replay.

Events are queued in the same database transaction as the change, so a change that is rolled back never fires an event, and one that is saved always does.

### Writing a good endpoint

- **Answer fast.** Check the signature, store the event, answer `200`, and do the real work afterwards. Slow work risks the 10 second timeout and a retry.
- **Expect duplicates.** A delivery can arrive more than once, for example when your answer is lost on the way back. Use `opensign-delivery` (the payload's `id`) to drop the ones you have already handled. A [resend](#delivery-log) has a new `id`, so when an event must never be handled twice, also check `event` and `data`, such as the `documentId` with the event name.
- **Do not rely on order.** Deliveries are independent, and a retried one can arrive after a later event. Use `createdAt`, or fetch the document's current status from the API, when order matters.
- **Answer `2xx` for events you ignore**, or they will be retried for an hour.

## Delivery log

Choose **Deliveries** on a webhook to see every event sent to it in the last 30 days, newest first. Filter by **Failed**, **Delivered** or **Pending**.

Each row shows the event, when it happened, and its state:

| State | Meaning |
| --- | --- |
| Pending | Queued, or waiting for the next retry after a failed attempt. The row shows when the next try is. |
| Delivered | Your endpoint answered `2xx`. |
| Failed | All 8 attempts failed and OpenSign gave up. |

Open a delivery to see the JSON payload that was posted and every attempt: when it ran, how long it took, the status code or error, and the first 2 KB of your endpoint's answer. OpenSign keeps the start of the answer so you can see what your endpoint said, such as an error page. Request headers are not kept: the signature is computed fresh for each attempt from the secret, which never appears in the log.

**Resend** sends a delivery again, whatever its state. The resend is a new delivery with its own `id` and a fresh signature, so it is not mistaken for a duplicate. Its `event`, `createdAt` and `data` are the same as the original's, and the two are linked in the log. **Resend Failed** does this for every failed delivery of the webhook that was not resent yet, for example after you fixed your endpoint. Each resend is recorded in the [audit trail](audit-log.md).

Deliveries older than 30 days are removed from the log. The log needs the **webhooks.manage** permission, since payloads hold document titles and references.

## Troubleshooting

| What you see | What to check |
| --- | --- |
| The webhook cannot be saved | The URL must be `https`, public, and without credentials. |
| `The endpoint answered 401` or `403` | Your endpoint refused the request. Check that it reads the raw body and uses the right secret. |
| `The endpoint answered 3xx` | Redirects are not followed. Use the final URL, including `https` and any trailing slash your server expects. |
| A timeout | Your endpoint took more than 10 seconds. Answer first, then do the work. |
| A refused address | The host resolves to a private or reserved address. Webhooks only go to the public internet. |
| Nothing arrives | Check that the webhook subscribes to the event, then open its **Deliveries**. A pending delivery shows its last error and the next try; a failed one can be resent once your endpoint is fixed. |
| Your endpoint answers `2xx` but rejects the event | Open the delivery and compare its payload and your endpoint's answer. Resend it once fixed. |
