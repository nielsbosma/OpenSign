# Automations

Automations send document data to the apps you already use, without writing code: save every signed PDF to Google Drive, post to a Slack channel when a contract is signed, update a HubSpot deal, add a row to a Google Sheet. They run through [Composio](https://composio.dev), which connects to hundreds of apps, with your own Composio account.

Each automation is a rule: **when** a document event happens, optionally only for some templates or folders, **run** one tool in a connected app, with inputs filled from the document. Runs are retried like [webhooks](webhooks.md) and every run is in a [log](#run-log).

For your own systems, [webhooks](webhooks.md) and the [REST API](api.md) are usually the better fit. Automations are for apps that you would otherwise connect with Zapier, Make or Power Automate.

## Turning it on

Automations are off until an admin turns them on.

1. Create a project at [composio.dev](https://composio.dev) and copy its API key from the project's settings.
2. Open **Settings → Automations**, paste the key into **Composio API Key** and switch on **Turn On Automations**. Save.

| Field | Variable | |
| --- | --- | --- |
| Turn On Automations | `OPENSIGN_AUTOMATIONS_ENABLED` | Off by default. Off: no automation runs and no app can be connected. Needs the key. |
| Composio API Key | `OPENSIGN_AUTOMATIONS_COMPOSIO_API_KEY` | Secret. Your Composio project's API key. |

The key is stored encrypted like every other secret setting, is never shown again and is only ever sent to Composio (`backend.composio.dev`). It is not available to API keys, MCP clients or the AI assistant. Each install uses its own key, so your connections and data stay in your own Composio project.

Managing automations needs the **automations.manage** permission, which only admins have. Every change is in the [audit trail](audit-log.md).

## Connecting apps

Under **Connected Apps**, choose **Connect an App**, search for the app and choose **Connect**. You sign in to the app on a page Composio shows, and come back to OpenSign when the account is connected. Its status then shows **ACTIVE**.

- Connections belong to the organisation, not to the person who connected them, so automations keep working when someone leaves. In Composio they are all under one user that stands for this install.
- A connection whose sign-in expired shows **EXPIRED** or **FAILED**. Choose **Reconnect** to sign in again.
- **Disconnect** removes the account from Composio and pauses every automation that uses it.

The account's tokens stay at Composio. OpenSign never sees or stores them.

## Rules

Choose **New Automation**, or start from a recipe:

| Recipe | What it does |
| --- | --- |
| Save the Signed PDF to Google Drive | Uploads the sealed PDF when everyone has signed. |
| Save the Signed PDF to Dropbox | The same, to a Dropbox folder. |
| Post to a Slack Channel | Posts "Signed: title by parties" with a link to the document. |
| Update a HubSpot Deal | Updates the deal whose id is the document's reference. |
| Add a Row to Google Sheets | Appends a row with the values you pick. |

A rule has:

- **Name**, for the list and the log.
- **When**: one of the events below.
- **App**: a connected account.
- **Tool**: what the app should do, such as uploading a file or sending a message. The list comes from Composio, and the form below it is built from the tool's real inputs. Required inputs are marked with `*`.
- **Inputs**: text, with [placeholders](#placeholders) that are filled from the document when the rule runs. A file input can take **The sealed PDF**.
- **Only for These Templates** and **Only in These Folders**: tick some to run the rule only for documents made from those templates or kept in those folders. None ticked: every document.

When you save, OpenSign checks the rule with Composio: the connection must be yours and active, the tool must belong to its app, every input must be one the tool has, and every required input must be filled in. The tool's version is saved with the rule, so a later change at Composio cannot change what the rule does without you noticing.

Use the switch on a rule to pause and resume it. **Test** runs the rule once, now, with a document you pick; the app really receives the data. **Delete** removes the rule and its run history.

### Events

| When | Event |
| --- | --- |
| A document is created | `document.created` |
| A document is sent | `document.sent` |
| A party opens their link | `document.viewed` |
| A party signs | `document.signed` |
| Everyone has signed | `document.completed` |
| A party declines | `document.declined` |
| A document is canceled | `document.canceled` |
| A document expires | `document.expired` |

The sealed PDF exists only once a document is completed, so rules that send it use `document.completed`.

## Placeholders

Write placeholders in double braces in any input. A value that is only a placeholder keeps its type (a number stays a number, a list stays a list, the sealed PDF becomes a file); inside other text each placeholder is written out.

| Placeholder | |
| --- | --- |
| `{{document.title}}` | The document's title |
| `{{document.reference}}` | Your own reference (external id), if one was given |
| `{{document.id}}`, `{{document.status}}` | Its id and status |
| `{{document.url}}` | A link to the document in OpenSign, for people with access |
| `{{document.createdAt}}`, `{{document.sentAt}}`, `{{document.completedAt}}` | Times, in ISO 8601 |
| `{{document.sealedSha256}}` | The SHA-256 of the sealed PDF |
| `{{document.valuesUrl}}` | The API address of its [filled-in values](data-export.md) |
| `{{fields.Label}}` | A field's value, by its label. Labels used by several parties get the party, as in `{{fields.Name (Party 2)}}`, exactly as in the [data export](data-export.md) |
| `{{parties}}` | Every party's name, comma separated, or the whole list when it is the only thing in the input |
| `{{parties.0.email}}` | One party, counted from 0 in signing order: `name`, `email`, `phone`, `company`, `role`, `status`, `slot`, `signedAt`, `declinedAt` |
| `{{party.name}}`, `{{party.email}}` | The party the event is about, for signed, viewed and declined |
| `{{declineReason}}` | Why the party declined |
| `{{owner.name}}`, `{{owner.email}}` | The person who sent the document |
| `{{event}}`, `{{eventTime}}` | The event and when it happened |
| `{{sealedPdf}}` | The sealed PDF as a file, for a tool's file input. Only on `document.completed` |

Field values are typed like the [data export](data-export.md): numbers as numbers, checkboxes as true or false, several choices as a list, uploads by file name. Personal numbers, signatures and identification evidence are never available to automations. A placeholder OpenSign does not know is refused when you save the rule.

## Run log

**Runs** on a rule lists its runs from the last 30 days, newest first, like the [webhook delivery log](webhooks.md#delivery-log). Open one to see each attempt: when, how long it took, what Composio answered (the first 2 KB) and the event it ran for.

- A run that fails because Composio or the app could not be reached, or answered with a server error, is tried again after 30 seconds, 1 minute, 2 minutes and so on, like webhooks.
- A run that fails because the input was refused (the app answered that something is missing or wrong, or the key was refused) is not retried. Fix the rule, then choose **Run Again** or **Run Failed Again**.
- A run of a paused rule, or while automations are off, does not call Composio.
- **Run Again** runs the same event again, as a new run, with the document's data as it is now.

Every run is also written to the [audit trail](audit-log.md) as `automations.run` on the document, with the app, the tool and the placeholders whose data was sent.

## With the API

Automations can also be managed with the [REST API](api.md) and the [MCP server](mcp.md), by an admin, with the `automations.manage` scope. The Composio API key itself is only set in **Settings → Automations**.

| Request | |
| --- | --- |
| `GET /api/v1/automations` | List rules with the result of their latest run. |
| `POST /api/v1/automations` | Add a rule (see below). |
| `PUT /api/v1/automations/{id}` | Change a rule; send the whole rule. `"active": false` pauses it. |
| `DELETE /api/v1/automations/{id}` | Remove a rule and its runs. |
| `POST /api/v1/automations/{id}/test` | Run it now for `{"documentId": "…"}`. |
| `GET /api/v1/automations/{automationId}/runs` | Its runs. Filter with `status` (`pending`, `delivered`, `failed`); page with `before` and `limit`. |
| `GET /api/v1/automation-runs/{id}` | One run with every attempt. |
| `POST /api/v1/automation-runs/{id}/resend` | Run it again. |
| `POST /api/v1/automations/{automationId}/resend-failed` | Run again every given-up run, up to 500. |

Through RPC and MCP there are also `automations.status`, `automations.apps` (search apps), `automations.connections`, `automations.connect` (returns the link to sign in at), `automations.disconnect`, `automations.tools` (what an app can do) and `automations.tool` (a tool's inputs as a JSON schema).

```json
{
  "name": "Signed contracts to Drive",
  "event": "document.completed",
  "templateIds": ["0192f0c4-…"],
  "connectionId": "ca_…",
  "tool": "GOOGLEDRIVE_UPLOAD_FILE",
  "arguments": { "file_to_upload": "{{sealedPdf}}" }
}
```

## Security

- Automations are off by default and only admins can set them up.
- OpenSign talks only to `backend.composio.dev`, and uploads the sealed PDF to the storage address Composio gives for it, which must be a public https address. Nothing else is reachable through a rule.
- A rule can only use connections of this install's Composio user, so a shared Composio project cannot reach another install's accounts.
- What a rule sends is limited to the placeholders above. The audit trail records each run and which data it sent.
- Tool answers are kept for 30 days in the run log, which only admins can read.
