# Exporting filled-in data

What signers type into fields ends up in the sealed PDF, and you can also get it out as data: as a spreadsheet, as JSON, from the API, or from an AI agent over MCP.

## From the app

Choose **Export Data**, then **Excel**, **CSV** or **JSON**:

- On the **Documents** page, next to **New Document**. The export holds the documents the list shows: the status tab, folder, search and archive tab you have chosen, not only the page you see.
- With rows selected on the **Documents** page, under **Actions**, for just those documents.
- On a template, in the **⋯** menu, for every document made from it.
- On a bulk send, for every document it created.
- On a document, in the **⋯** menu, for that one document.

One export holds up to 5,000 documents. Narrow the filters to export more in parts.

## What is in it

A spreadsheet has one row per document, oldest first:

| Columns | |
| --- | --- |
| Document ID, Title, Reference, Status | The document. Reference is your **Your Reference**. |
| Created, Sent, Completed | Times in UTC. |
| Party 1 Name, Party 1 Email, Party 1 Signed | Each party in signing order, then Party 2 and so on. |
| One column per field | Named after the field's label. |

Field values are written the way OpenSign stores them, so they sort and sum in a spreadsheet:

| Field | Value |
| --- | --- |
| Number | A plain number, such as `1234.5`, whatever display format the field has. In Excel it is a real number. |
| Date picker and date of signing | `YYYY-MM-DD`. |
| Checkbox | `true` or `false`. In Excel a real TRUE or FALSE. |
| Dropdown and radio group | The chosen option. A radio group is one column, named after the group's label. |
| Multiple choice | The chosen options, separated by commas (a list in JSON). |
| Phone | The number with its country code, such as `+46701234567`. |
| Image and file upload | The file name. The file itself is not exported; download it from the document. |
| Signature | Left out: the party's Signed column says when they signed. |

A field that is not filled in yet is empty, so pending documents can be exported too.

### Column names

A column is named after the field's label, or after its kind when it has none. When parties have fields with the same label, such as a Name field each, the party is added: `Name (Party 1)`, `Name (Party 2)`. A label used twice by the same party gets a number: `Note`, `Note (2)`. Give fields clear, unique labels in the template and the columns follow.

When you export documents from different templates, every field label becomes a column and a document without that field leaves it empty.

## Safety

- Only documents you can see are exported. People without access to every document export their own and those shared with them.
- Exporting needs the `documents.export` permission, which admins, managers, senders and auditors have. Viewers can see a shared document's values on the document page, but cannot export. See [Users and roles](users-and-roles.md#permissions).
- Personal numbers are never exported, and uploaded files are listed by name only.
- A CSV cell that starts with `=`, `+`, `-` or `@` gets a leading `'` so that a spreadsheet shows it as text instead of running it as a formula. Otherwise a signer could type `=HYPERLINK(…)` into a field and have it run on your computer. Negative numbers are left alone. Excel files write text as text, so nothing needs changing there.
- CSV files are UTF-8 with a byte order mark, so Excel shows å, ü and 中 correctly.
- Every export is written to the [audit log](audit-log.md) as `documents.exported`, with the format, the filters and the documents it held.

## From the API

One document's values, typed:

```sh
curl https://sign.example.com/api/v1/documents/$ID/values -H "Authorization: Bearer $KEY"
```

```json
{
  "id": "019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28",
  "title": "Lease, Storgatan 1",
  "status": "completed",
  "parties": [{ "id": "…", "slot": "Party 1", "role": "signer", "name": "Anna Berg", "email": "anna@example.com", "signedAt": "2026-10-29T10:12:00Z", "…": "…" }],
  "fields": [{ "key": "Monthly rent", "label": "Monthly rent", "kind": "number", "partyId": "…", "fieldIds": ["…"], "value": 12500 }],
  "values": { "Monthly rent": 12500, "Pets": "No", "Rooms": ["Kitchen", "Bath"], "Agree to terms": true }
}
```

`values` is the quickest way in: field keys mapped to typed values. `fields` has the same values with their field ids, kinds and parties. Reading values needs only access to the document, and is not audited, like opening the document.

Many documents at once:

```sh
curl -X POST https://sign.example.com/api/v1/exports -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" -d '{"format": "xlsx", "templateId": "…", "status": "completed"}'
```

The body takes `format` (`csv`, `xlsx` or `json`) and any of `ids`, `templateId`, `bulkId`, `status`, `search`, `folderId` and `archived` (`true` for archived documents only, `false` to leave them out, left out for both). The answer has `name`, `contentType` and `count`, plus `contentBase64` for CSV and Excel or `documents` (each as above) for JSON.

DocuSign clients get the same values from `GET /envelopes/{id}/form_data`; see [DocuSign API](docusign.md).

Both calls are also MCP tools, `documents_values` and `documents_export`, so an agent can answer "how many tenants said yes to pets this month?" See [MCP](mcp.md).

## With webhooks

The `document.completed` event carries a `valuesUrl`, the `/values` address above, so an integration can fetch everything signers entered in one call. Webhooks themselves never carry field values. See [Webhooks](webhooks.md#events).
