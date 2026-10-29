# DocuSign compatibility

OpenSign answers the core of the DocuSign eSignature REST API v2.1, so an integration that creates envelopes, embeds signing and downloads the result can move to OpenSign by changing the base URL and the token.

| | |
| --- | --- |
| DocuSign | `https://demo.docusign.net/restapi/v2.1` (or your production host) |
| OpenSign | `https://<your-install>/docusign/restapi/v2.1` |
| Alias | `https://<your-install>/restapi/v2.1` |

## Authentication

Use an OpenSign [API key](api.md#api-keys) as the access token: `Authorization: Bearer osk_…`. There is no OAuth dance to replace. The `{accountId}` in paths is accepted and ignored, since an install holds one organisation.

## Endpoints

All paths are under `/accounts/{accountId}`.

| Method | Path | |
| --- | --- | --- |
| POST | `/envelopes` | Create from `documents[]` (base64) and `recipients`. `status: "sent"` sends at once |
| GET | `/envelopes` | List. `start_position`, `count` (up to 100), `status`, `search_text`, `from_date` |
| GET | `/envelopes/{id}` | One envelope |
| PUT | `/envelopes/{id}` | `status: "sent"` sends; `status: "voided"` with `voidedReason` cancels; otherwise `emailSubject` renames |
| GET | `/envelopes/{id}/recipients` | Signers and carbon copies with status and times |
| GET | `/envelopes/{id}/documents` | The envelope's documents |
| GET | `/envelopes/{id}/documents/{documentId}` | The whole envelope as one PDF, sealed with evidence pages once complete. Any document id returns it |
| POST | `/envelopes/{id}/views/recipient` | An embedded signing URL for a recipient added with `clientUserId`. Match by `recipientId`, or `email` plus `userName` |

## Recipients

- `signers` and `inPersonSigners` become signers. `carbonCopies` and `certifiedDeliveries` become viewers.
- `routingOrder` is the signing order, and envelopes sign in order.
- A recipient with `clientUserId` gets link delivery, for embedded signing through `views/recipient`.
- `smsAuthentication` or `phoneAuthentication` turns on SMS codes to view and sign.
- Your `recipientId` strings and `customFields.textCustomFields` are kept in the document's metadata and returned.

## Tabs

| Tab | OpenSign field | Default size (pt) |
| --- | --- | --- |
| `signHereTabs` | signature | 150 × 40 |
| `initialHereTabs` | initials | 60 × 30 |
| `dateSignedTabs` | date | 100 × 18 |
| `fullNameTabs` | name | 150 × 18 |
| `emailAddressTabs` | email | 180 × 18 |
| `companyTabs` | company | 150 × 18 |
| `textTabs`, `zipTabs` | text | 150 × 18, 80 × 18 |
| `dateTabs` | date picker | 100 × 18 |
| `numberTabs` | number | 100 × 18 |
| `listTabs` | dropdown, from `listItems` | 150 × 18 |
| `radioGroupTabs` | radio group, one button per entry in `radios` | 14 × 14 |
| `phoneNumberTabs` | phone | 130 × 18 |
| `signerAttachmentTabs` | file | 150 × 30 |
| `checkboxTabs` | checkbox | 14 × 14 |

`validationPattern`, `validationMessage` and `maxLength` become the field's [checks](documents.md#checks-and-error-texts), as do `minNumericalValue` and `maxNumericalValue` on number tabs. A `locked` tab with a value is read-only.

Tabs are placed by `documentId`, `pageNumber`, `xPosition` and `yPosition` in points, measured against the real page size of the uploaded file and clamped onto the page.

## Envelope statuses

| DocuSign | OpenSign |
| --- | --- |
| `created` | draft |
| `sent` | pending, nobody has opened it |
| `delivered` | pending, someone has opened it |
| `completed` | completed |
| `declined` | declined |
| `voided` | canceled or expired |

## Errors

```json
{ "errorCode": "ENVELOPE_INVALID_STATUS", "message": "Only pending documents can be canceled" }
```

Codes: `INVALID_REQUEST_PARAMETER`, `USER_AUTHENTICATION_FAILED`, `USER_LACKS_PERMISSIONS`, `ENVELOPE_DOES_NOT_EXIST`, `ENVELOPE_INVALID_STATUS`, `HOURLY_APIINVOCATION_LIMIT_EXCEEDED` and `UNSPECIFIED_ERROR`.

## What is not supported

These are refused with a clear `400` rather than guessed:

- anchor tabs (`anchorString`): place tabs by position instead,
- server templates (`templateId`, `compositeTemplates`): send the documents directly, or use OpenSign [templates](templates.md) through the native API,
- `identityVerification`: set BankID, Freja or another eID on the party in OpenSign. See [Identification](identification.md).

DocuSign Connect notifications are not sent; use OpenSign [webhooks](webhooks.md) instead.
