# Scrive compatibility

OpenSign answers the Scrive Document API v2, so an existing Scrive integration can move to OpenSign by changing the base URL and the credentials.

| | |
| --- | --- |
| Scrive | `https://scrive.com/api/v2` |
| OpenSign | `https://<your-install>/scrive/api/v2` |
| Alias | `https://<your-install>/api/v2` |

## Authentication

Create an OpenSign [API key](api.md#api-keys). Scrive clients sign requests with OAuth 1.0 PLAINTEXT; OpenSign reads the key from `oauth_token` in that header and ignores the other values, since the key alone carries the secret. A plain `Authorization: Bearer osk_…` works too. Cookies are never accepted on these endpoints.

The key's scopes and its owner's role apply as everywhere else.

## Endpoints

| Method | Path | |
| --- | --- | --- |
| POST | `/documents/new` | Create a draft; an optional multipart `file` becomes the file and its name the title |
| POST | `/documents/newfromtemplate/{id}` | Create from a template |
| GET | `/documents/list` | `offset`, `max` (up to 100), `filter` JSON with `status`, `text` or `template` |
| GET | `/documents/{id}/get` | The document in Scrive's shape |
| POST | `/documents/{id}/update` | Form field `document` (JSON): title, invitation_message, lang, days_to_sign, days_to_remind, parties with fields and placements |
| POST | `/documents/{id}/setfile` | Replace the files with the multipart `file` |
| POST | `/documents/{id}/start` | Send |
| POST | `/documents/{id}/cancel` | Cancel |
| POST | `/documents/{id}/remind` | Remind |
| POST | `/documents/{id}/trash`, `/delete` | Delete |
| GET | `/documents/{id}/files/main/{name}` | The PDF: sealed once closed, otherwise as sent, or the merged draft |

## How Scrive maps to OpenSign

| Scrive status | OpenSign |
| --- | --- |
| `preparation` | draft |
| `pending` | pending |
| `closed` | completed |
| `canceled` | canceled |
| `timedout` | expired |
| `rejected` | declined |

| Scrive | OpenSign |
| --- | --- |
| delivery `email`, `email_mobile`, `portal` | email |
| delivery `mobile` | sms |
| delivery `api` | link, returned as `api_delivery_url` |
| delivery `pad` | in person |
| authentication `standard` | standard (sign) or none (view) |
| authentication `se_bankid`, `sms_pin`, `freja` | the same |
| `signatory_role` signing_party, approver, viewer | signer, approver, viewer |
| `lang` | the document's signer language |

- Placements use Scrive's `xrel`, `yrel`, `wrel`, `hrel` and one-based `page`. Field types `signature`, `checkbox`, `name` (order 1), `email`, `company` and text-like types are placed. The author party's text fields become values the sender fills in.
- The list filter takes one status at a time.

## Errors

Errors use Scrive's shape:

```json
{ "error_type": "document_state_error", "error_message": "Only drafts can be changed", "http_code": 409 }
```

Types: `request_parameters_invalid`, `invalid_authorisation`, `insufficient_privileges`, `resource_not_found`, `document_state_error`, `request_failed` and `server_error`.

## What is different

Scrive's `api_callback_url` callbacks are not sent (the field is always `null`); use OpenSign [webhooks](webhooks.md) instead. Anything else your integration needs that is missing is worth an issue on GitHub.
