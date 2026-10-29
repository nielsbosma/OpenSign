# REST API

Everything the web app does runs through one set of services, and the REST API sits directly on top of them. Permissions, validation and the audit trail are the same as in the app.

| | |
| --- | --- |
| Base URL | `https://<your-install>/api/v1` |
| Format | JSON, camelCase field names |
| Authentication | `Authorization: Bearer osk_…` |
| OpenAPI | `GET /api/v1/openapi.json` (OpenAPI 3.1, generated from the service schemas) |
| Try it | **API Explorer** in the user menu at the bottom of the sidebar |

Besides the native API, OpenSign offers drop-in [Scrive](scrive.md) and [DocuSign](docusign.md) endpoints, [webhooks](webhooks.md), and an [MCP server](mcp.md).

## API Explorer

Signed in to the app, **API Explorer** lists every endpoint with its parameters and schemas, sends live requests as you or with a pasted API key, and writes the same call as curl, JavaScript or Python. It reads the same OpenAPI document as your tools would.

## API keys

Create keys under **Settings → API Keys**. Creating a key needs a signed-in user; a key cannot create another key.

| Option | |
| --- | --- |
| Name | To tell keys apart, such as "CRM integration". |
| Scopes | The permissions the key carries. See below. |
| Expires After (Days) | Empty for no expiry. |
| Allowed IPs | Addresses or CIDR ranges the key may be used from, such as `203.0.113.0/24`. Empty for any. From any other address the key is treated as invalid. |

The key is shown once, as `osk_<prefix>_<secret>`. OpenSign stores only its hash. **Revoke** stops it working on the next request.

At each request a key gets the **intersection** of its scopes and its owner's current role. Demoting the owner narrows every key they made, and deactivating the owner revokes their keys.

**Edit** changes a key's name, scopes and allowed IPs. The key itself stays the same and is never shown again, and the change applies from the next request. You can only give a key scopes your own role has. Through an API key or MCP a key can only be narrowed: fewer scopes, or a tighter IP list. Adding scopes or loosening the IP list needs a signed-in user. Each edit is in the audit log with the values before and after. The expiry cannot be changed; create a new key instead.

### Scopes

| Scope | Allows |
| --- | --- |
| `documents.read` | See your own documents and those shared with you |
| `documents.create` | Upload documents and send them for signing |
| `documents.read_all` | See every document |
| `documents.manage_all` | Change, cancel and delete any document |
| `documents.legal_hold` | Place and release legal holds |
| `templates.manage` | Create and change shared templates |
| `webhooks.manage` | Add and remove webhooks |
| `users.read` | See who has an account |
| `users.manage` | Invite people, change roles and deactivate accounts |
| `audit.read` | Browse and verify the audit log |
| `org.read` | See organisation details |
| `ai.use` | Use the AI assistant and document review |
| `system.read` | See versions, migrations and background jobs |
| `settings.manage` | Read settings, the seal certificate, AI usage and the retention preview |
| `api_keys.manage` | List, narrow and revoke your keys |

### App-only calls

Some things can only be done signed in to the app, never with a key: signing in, passwords, two-factor and passkey setup, creating API keys, changing settings, the in-app update, replacing the seal key, starting in-person signing, and everything a signer does. They answer `403` with "This call is only available signed in to the app".

## Conventions

- Path parameters and query parameters are merged into the same input as the body. `PATCH /documents/{id}` takes `id` from the path and the rest from the body.
- On `GET`, query strings are typed by the input schema: `limit=20` is a number and `templates=true` a boolean. A value that does not parse is a `400`.
- A `POST` that creates something answers `201`; everything else answers `200`. Responses carry `Cache-Control: no-store`.
- Every response has an `x-request-id` header. Internal errors quote it, so you can find the cause in the server log.

### Errors

```http
HTTP/1.1 409 Conflict
x-request-id: 3f9a1c0be27d

{ "error": "Only drafts can be changed", "kind": "conflict" }
```

| `kind` | Status |
| --- | --- |
| `bad_request` | 400 |
| `unauthorized` | 401 |
| `forbidden` | 403 |
| `not_found` | 404 |
| `conflict` | 409 |
| `too_many_requests` | 429 |
| `unavailable` | 503 |
| `internal` | 500 |

A bearer token that does not check out is always a `401`, never silently ignored.

### Idempotency

Every write accepts an `Idempotency-Key` header of 1 to 200 characters. A retry with the same key, method, path and input gets the first response back, with `Idempotent-Replayed: true`, instead of acting twice.

- Keys are scoped to the API key that used them and kept for a day.
- The same key with a different request, or while the first request is still running, is a `409`.
- Validation and permission errors are stored and replayed like successes. Internal errors are not stored, so a retry runs again.
- Stored responses are encrypted, since they can hold signing links and webhook secrets.

## Send a document

```sh
OS=https://sign.example.com
KEY=osk_...

# 1. Create a draft
curl -X POST $OS/api/v1/documents -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" -H "Idempotency-Key: crm-4711" \
  -d '{"title":"Consulting agreement","externalId":"crm-4711","language":"sv"}'

# 2. Add a PDF or Word file
curl -X POST $OS/api/v1/documents/$ID/files -H "Authorization: Bearer $KEY" \
  -F file=@agreement.docx

# 3. Set the parties (the complete list)
curl -X PUT $OS/api/v1/documents/$ID/parties -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" -d '{"parties":[
    {"name":"Anna Berg","email":"anna@example.com","authToSign":"se_bankid",
     "personalNumber":"199001011234"}]}'

# 4. Place fields, as fractions of the page, origin top-left
curl -X PUT $OS/api/v1/documents/$ID/fields -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" -d '{"fields":[
    {"partyId":"'$PARTY'","kind":"signature","page":0,
     "x":0.12,"y":0.78,"width":0.3,"height":0.06}]}'

# 5. Send
curl -X POST $OS/api/v1/documents/$ID/send -H "Authorization: Bearer $KEY"

# 6. Later, download the sealed PDF
curl -o signed.pdf "$OS/api/v1/documents/$ID/file?version=signed" -H "Authorization: Bearer $KEY"
```

Send answers `{ "document": {…}, "links": [{ "partyId", "name", "url" }] }`. The links are for parties with `delivery: "link"`, for you to hand over. Use a [webhook](webhooks.md) for `document.completed` instead of polling.

## Endpoints

### Account

| Method | Path | |
| --- | --- | --- |
| GET | `/me` | Who the caller is, their role and effective permissions |
| GET | `/api-keys` | List your keys, never the keys themselves |
| PATCH | `/api-keys/{id}` | Change a key. Body: any of `name`, `permissions`, `allowedIps`. With a key, only narrowing is allowed |
| DELETE | `/api-keys/{id}` | Revoke a key |

### Documents

| Method | Path | |
| --- | --- | --- |
| GET | `/documents` | List. Query: `status`, `search` (title or exact external id), `templates`, `folderId`, `archived`, `limit`, `offset`. Archived ones are listed only with `archived=true` |
| POST | `/documents` | Create a draft or template. Body: `title`, `message`, `language`, `templateId`, `isTemplate`, `sequential`, `externalId`, `metadata`, `folderId`. A document made from a template has `templateId` and `templateVersion`; `isTemplate` with `templateId` duplicates a template |
| GET | `/templates` | List templates |
| GET | `/documents/{id}` | One document with files, parties and fields |
| PATCH | `/documents/{id}` | Change `title`, `message`, `language`, `sequential`, `expiresAt`, `reminderDays` (0 turns reminders off), `externalId`, `metadata` |
| DELETE | `/documents/{id}` | Delete a document or template that is not pending, with its files |
| POST | `/documents/{id}/files` | Upload one file as multipart `file`: PDF, .docx, .doc, .odt or .rtf. [Text tags and form fields](text-tags.md) become fields unless `textTags` or `formFields` is `false` |
| DELETE | `/documents/{documentId}/files/{fileId}` | Remove a file from a draft |
| POST | `/documents/{id}/files/{fileId}/replace` | Replace a file with a new one (multipart `file`) without redoing its fields. Returns `moved`: fields that were on pages the new file lacks, now on its last page |
| PUT | `/documents/{documentId}/parties` | Replace the party list. Parties left out are removed with their fields; pass `id` to keep one |
| PUT | `/documents/{documentId}/fields` | Replace the fields |
| POST | `/documents/{id}/send` | Send for signing. Optional `expiresAt` |
| POST | `/documents/{id}/cancel` | Cancel a pending document. Optional `reason` |
| POST | `/documents/{id}/remind` | Remind parties whose turn it is |
| POST | `/documents/{id}/shares` | Share with a colleague. Body: `userId`, `canEdit` |
| DELETE | `/documents/{id}/shares/{userId}` | Stop sharing |
| POST | `/documents/{documentId}/parties/{partyId}/signing-link` | Issue a signing link to hand over. `rotate: true` revokes earlier links first; `redirectUrl` sends the party to your site afterwards (see [Embedded signing](embedding.md#redirect-after-signing)) |
| PUT | `/documents/{id}/legal-hold` | Body `hold`, `reason`. See [Retention and legal hold](retention.md) |
| GET | `/documents/{id}/file` | The PDF. `version=signed` (sealed, completed only) or `original` (as sent) |
| GET | `/templates/{templateId}/versions` | A template's versions, newest first, and `changed`: whether it changed since the newest |
| POST | `/templates/{templateId}/versions` | Save the template as a new version. Body: `note` |
| GET | `/templates/{templateId}/versions/{number}` | One version: its settings, files, parties and fields, and the documents made from it |
| GET | `/templates/{templateId}/compare` | What changed between two versions. Query: `from`, `to` (leave one out for the template as it is now) |
| POST | `/templates/{templateId}/versions/{number}/restore` | Put the template back to a version; the restore is saved as a new version |
| POST | `/documents/move` | Move documents or templates to a folder. Body: `ids`, `folderId` (`null` for none) |
| POST | `/documents/archive` | Archive finished documents or templates, or restore them. Body: `ids`, `archived` |
| GET | `/folders` | List folders. Query: `scope` (`documents` or `templates`) |
| POST | `/folders` | Create a folder. Body: `scope`, `name`, `parentId` |
| PUT | `/folders/{id}` | Rename or move a folder. Body: `name`, `parentId` |
| DELETE | `/folders/{id}` | Delete a folder; what was in it moves up a level |

Editing calls work on drafts and templates. Once a document is sent they answer `409`. Moving to a folder and archiving work at any status.

### Bulk send

| Method | Path | |
| --- | --- | --- |
| GET | `/bulk-sends` | Recent bulk sends you started, or all with `documents.read_all` |
| POST | `/bulk-sends/preview` | Read a CSV, Excel or OpenDocument file for a template. Body `templateId`, `fileName`, `contentBase64`, `useAi`. Returns columns, sample rows, targets, a suggested mapping and rows that would fail. Sends nothing |
| POST | `/bulk-sends` | Send the template to every row (up to 1,000). Body as preview plus `mapping: [{ column, target }]` and `titlePattern`. Refuses the whole file if any row cannot be sent |
| GET | `/bulk-sends/{id}` | One bulk send with every row's outcome |
| POST | `/bulk-sends/{id}/cancel` | Skip rows not sent yet |

See [Bulk send](bulk-send.md).

### Webhooks

| Method | Path | |
| --- | --- | --- |
| GET | `/webhooks` | List webhooks with the latest delivery result |
| POST | `/webhooks` | Add one. Body: `url`, `events`. Returns the signing secret once |
| DELETE | `/webhooks/{id}` | Remove one; queued deliveries are dropped |
| POST | `/webhooks/{id}/test` | Send a `webhook.test` event |
| GET | `/webhooks/{webhookId}/deliveries` | The delivery log, newest first. Query: `status` (`pending`, `delivered`, `failed`), `event`, `before`, `limit` |
| GET | `/webhook-deliveries/{id}` | One delivery with its payload and every attempt |
| POST | `/webhook-deliveries/{id}/resend` | Send a delivery again, as a new delivery |
| POST | `/webhooks/{webhookId}/resend-failed` | Resend every delivery that was given up |

See [Webhooks](webhooks.md).

### Organisation and system

| Method | Path | |
| --- | --- | --- |
| GET | `/users` | List accounts |
| GET | `/audit` | Browse the audit trail |
| GET | `/audit/verify` | Verify the audit trail's hash chain |
| GET | `/retention/preview` | What the retention rules would delete at the next run |
| GET | `/system/version` | The running version and the latest release. `refresh=true` checks now |
| GET | `/system/status` | Database version, migrations and the job queue |
| POST | `/verify` | Verify a signed PDF (raw body, up to 40 MB). Needs no key |

### Health

`GET /api/health` returns `{"ok":true,"version":"…"}` and `GET /api/ready` returns `{"ready":true}` once the database answers (503 otherwise). Neither needs a key.

## Parties

| Field | |
| --- | --- |
| `id` | Pass to keep an existing party and its fields |
| `role` | `signer` (default), `approver`, `viewer` |
| `name` | Required |
| `email`, `phone`, `company` | `phone` with country code, such as `+46701234567` |
| `personalNumber` | Matched against the eID. Stored encrypted. Omit or send the masked value to keep it; `""` removes it |
| `signOrder` | Lower numbers go first in sequential documents; equal numbers sign in parallel |
| `delivery` | `email` (default), `sms`, `link`, `in_person`, `none` |
| `authToView` | `none`, `sms_pin`, `se_bankid`, `freja`, `broker` |
| `authToSign` | `standard`, `sms_pin`, `se_bankid`, `freja`, `broker` |
| `attachments` | Files to ask for: `[{ "name", "description", "required" }]` |

## Fields

| Field | |
| --- | --- |
| `partyId` | Who fills it in. Omit for a value the sender fills in now |
| `kind` | `signature`, `initials`, `text`, `number`, `select`, `multiselect`, `radio`, `checkbox`, `phone`, `date_input`, `image`, `file`, `formula`, and the automatic `date` (of signing), `name`, `email`, `company` |
| `page` | Zero-based page in the combined document (all files in order) |
| `x`, `y` | Top-left corner as fractions of the page (0 to 1), origin top-left |
| `width`, `height` | Fractions of the page |
| `required`, `label` | Optional |
| `value` | A prefilled value: `"true"` ticks a checkbox, numbers as `1234.5`, dates as `2026-12-31`, a multiselect as a JSON array such as `"[\"A\",\"C\"]"`, a radio button as its option when chosen |
| `options` | The choices of a `select` or `multiselect`, or the one option of a `radio` button |
| `groupKey` | Ties `radio` buttons into one group; every button of the group has the same key |
| `validation` | `{ "pattern", "minLength", "maxLength", "min", "max", "message" }`, see [Checks](documents.md#checks-and-error-texts) |
| `format` | How a number or date is written on the page, see [formats](documents.md#number-and-date-formats) |
| `readOnly` | The party sees the value but cannot change it |
| `conditions` | Show the field only when all of these hold: `[{ "field": "Type", "op": "equals", "value": "Company" }]`. `field` is another field's label; `op` is `equals`, `not_equals`, `filled`, `empty`, `contains`, `greater` or `less`. See [Conditional fields](documents.md#conditional-fields) |
| `formula` | For a `formula` field, such as `{Hours} * {Rate} * 1.25`. See [Formulas](documents.md#formulas) |

Values the signer gives come back normalised: numbers as plain decimals, phone numbers in E.164, dates as YYYY-MM-DD and multi-select choices as a JSON array. Image and file fields hold the name of the upload. A formula holds the number the server computed, and a field its conditions hid is empty.

Parties upload to an image or file field with `POST /api/sign/field-file` (multipart with `token`, `session`, `field` and `file`) from their signing page; `signing.remove_field_file` empties it again.

## Calls by name

The web app calls services by name at `POST /api/rpc/{service}` with a JSON body. An API key works there too, for every service that is also an MCP tool. That reaches the few services without a REST route, such as `documents.check` (check a draft before sending) and `documents.auth_methods` (which identification methods are set up). Prefer REST where a route exists: this route has no idempotency keys and is not in the OpenAPI document.

```sh
curl -X POST $OS/api/rpc/documents.check -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" -d '{"id":"'$ID'"}'
```

## Limits

Rate limits are per client address, as a token bucket:

| Bucket | Per minute | Burst | Applies to |
| --- | --- | --- | --- |
| general | 600 | 120 | All of `/api`, the DocuSign endpoints and `/mcp` |
| upload | 30 | 10 | File uploads |
| verify | 10 | 10 | `POST /verify` |
| oauth | 30 | 20 | OAuth token and revoke |
| auth | 10 | 10 | Sign-in and client registration |

| Body | Limit |
| --- | --- |
| JSON body | 40 MB |
| Multipart upload | 110 MB (and the install's **Largest Upload** setting) |
| PDF to verify | 40 MB |
| Scrive and DocuSign bodies | 60 MB |
| MCP and `/api/rpc` | 40 MB |
| Bulk send file | 1,000 rows |
