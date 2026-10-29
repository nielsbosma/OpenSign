# MCP and Claude

OpenSign has a built-in [Model Context Protocol](https://modelcontextprotocol.io) server. Connect Claude or another MCP client, and it can list, prepare, send and follow up documents for you, with your permissions and in the audit trail under your name.

## Connecting Claude

1. In OpenSign, open **Settings → Account** and copy the server URL under **AI Apps (MCP)**. It is `https://<your-install>/mcp`.
2. In Claude, add a custom connector with that URL. Other MCP clients, such as Claude Code, Cursor or ChatGPT, take the same URL.
3. Claude sends you to OpenSign to sign in and approve the connection on a consent page.

That is all. You can see and disconnect connected apps under **AI Apps (MCP)** at any time.

Clients that only take a static header can send an [API key](api.md#api-keys) as `Authorization: Bearer osk_…` instead of signing in.

If two-factor sign-in is required on the install and you have not set it up yet, connections are refused until you do.

## Transport

- Streamable HTTP with JSON responses and no sessions. `POST /mcp` takes one JSON-RPC message or a batch of up to 50.
- Protocol versions `2025-06-18` (default), `2025-03-26` and `2024-11-05`.
- Methods: `initialize`, `ping`, `tools/list`, `tools/call`, and empty `resources/list` and `prompts/list`.
- Requests from a browser with an `Origin` other than the install are refused.

## Sign-in (OAuth 2.1)

| Endpoint | |
| --- | --- |
| `/.well-known/oauth-protected-resource` | RFC 9728 resource metadata. A `401` points here |
| `/.well-known/oauth-authorization-server` | RFC 8414 server metadata |
| `POST /oauth/register` | Dynamic client registration (public clients, no secret) |
| `GET /oauth/authorize` | Authorization code with PKCE (S256 only); the user approves on a consent page |
| `POST /oauth/token` | `authorization_code` and `refresh_token` grants |
| `POST /oauth/revoke` | Token revocation |

The scope is `opensign`. Codes live 5 minutes, access tokens 1 hour and refresh tokens 30 days. Refresh tokens rotate, and reusing an old one revokes the whole grant. Redirect URIs must match exactly.

## Tools

Each tool is a service the connected user's role allows, named with underscores (`documents.send` becomes `documents_send`). An API key narrows that further to its scopes. Results come back as JSON text plus `structuredContent`. Every call is checked and audited exactly as in the app, with `mcp` as the source.

Tools carry the standard hints, so clients can ask before running them:

- **Read-only**: lists, lookups and downloads.
- **Destructive**: deletes, cancels, revokes, user and settings changes, and the in-app update. Marked ◆ below.
- **Open-world**: calls that reach outside OpenSign, such as email, SMS, webhooks, the AI provider or the deploy hook. Marked ↗ below.

| Area | Tools |
| --- | --- |
| Documents | `documents_list`, `documents_counts`, `documents_get`, `documents_create`, `documents_upload`, `documents_upload_url`, `documents_remove_file` ◆, `documents_update`, `documents_set_parties`, `documents_set_fields`, `documents_detect_fields` ↗, `documents_set_review_links`, `documents_check`, `documents_auth_methods`, `documents_signing_preview`, `documents_send` ↗, `documents_remind` ↗, `documents_cancel` ◆, `documents_delete` ◆, `documents_share`, `documents_unshare` ◆, `documents_signing_link`, `documents_download`, `documents_download_url`, `documents_verify`, `documents_legal_hold` ◆, `documents_move`, `documents_archive` |
| Folders | `folders_list`, `folders_create`, `folders_update`, `folders_delete` ◆ |
| AI review | `documents_review` ↗, `documents_latest_review`, `ai_status`, `ai_usage`, `ai_models` ↗, `ai_test` ↗ |
| Bulk send | `bulk_preview`, `bulk_start` ↗, `bulk_list`, `bulk_get`, `bulk_cancel` ◆ |
| Webhooks | `webhooks_list`, `webhooks_create` ↗, `webhooks_delete` ◆, `webhooks_test` ↗, `webhooks_deliveries`, `webhooks_delivery`, `webhooks_resend` ↗, `webhooks_resend_failed` ↗ |
| Users | `users_list`, `users_invite` ↗, `users_list_invites`, `users_revoke_invite` ◆, `users_update` ◆, `users_change_email` ◆ ↗ |
| Audit | `audit_list`, `audit_verify` |
| Account and keys | `account_me`, `account_set_language`, `api_keys_list`, `api_keys_update`, `api_keys_revoke` ◆ |
| Settings and system | `settings_get`, `settings_update` ◆, `settings_send_test_email` ↗, `branding_set_logo`, `branding_remove_logo` ◆, `seal_get`, `retention_preview`, `system_version`, `system_status`, `system_update` ◆ ↗ |
| Documentation | `docs_search`, `docs_read` |

### Settings and secrets

`settings_update` changes the same [Admin settings](settings.md) as the app, but a secret never passes through an agent:

- Secret fields (API keys, passwords, the SMTP password, eID certificates and client secrets, the deploy hook URL) can be **cleared** with an empty string, never set. Set them signed in to OpenSign.
- While a secret is saved, the fields that decide where it is sent cannot change, such as the AI provider's base URL or the SMTP host. Otherwise an agent could point a saved key at another server. Change them in the app, or clear the secret in the same call.
- `ai_models` lists models with the saved key only. A key typed into the call is refused.

The same rules apply to API keys calling the API, since an API key is just as likely to sit in a script or a chat.

### Big files

`documents_upload` and `documents_download` move files as base64, up to 20 MB, which works for small files. For bigger files, or for agents that can run a command:

- `documents_upload_url` returns a link to `PUT` the file's raw bytes to, such as `curl -T Agreement.pdf <url>`. It is tied to one draft and one file name, works once, and expires after 15 minutes. The answer is the same as `documents_upload`, and the upload is audited as the connected user.
- `documents_download_url` returns a link to `GET` a document's signed or original PDF. It expires after 10 minutes and stops working if the document is deleted. Creating it is audited as a download.

These links are the only credential needed, so an agent should hand them only to the program doing the transfer.

### Documentation tools

`docs_search` finds sections of these pages, and `docs_read` returns a page or one section as Markdown, so an agent can look up how a feature works before it acts. `docs_read` without a page lists every page.

## Not offered as tools

Some things stay in the app on purpose, because they are credentials, need the person present, or would let an agent widen its own access:

| Not a tool | Why |
| --- | --- |
| Signing in and out, passwords, two-factor and passkey setup | Credentials never pass through an agent. |
| Creating API keys | The new key would land in the agent's context. Editing and revoking keys are tools. |
| Resetting someone's two-factor | It weakens their sign-in, so an admin does it in the app. |
| OAuth consent and connected AI apps | Consent is given by the person in the browser, and an agent cannot see or change which clients hold access. |
| Replacing the seal key | Done in the app. |
| In-person signing and anything a signer does | Needs the signer at the device, with their own link. |
| The in-app AI assistant | Agents call the tools directly. |

Every service in OpenSign states in its definition whether it is a tool, and if not, why. A test fails when a new one makes no choice, so new features reach agents from day one.

The in-app [AI assistant](ai.md) uses the same tools minus a few that change who has access or where data goes, because document titles and party names it reads are written by other people. See [The assistant](ai.md#the-assistant).

## Things to try

- "Which documents have been waiting for signatures for more than a week? Remind those parties."
- "Create a draft from the NDA template for Anna Berg at Acme, anna@acme.example, with BankID to sign."
- "Has the Acme consulting agreement been signed? Download the sealed PDF."
- "Verify the hash chain of the audit log."
