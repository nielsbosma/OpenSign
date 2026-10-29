# MCP and Claude

OpenSign has a built-in [Model Context Protocol](https://modelcontextprotocol.io) server. Connect Claude or another MCP client, and it can list, prepare, send and follow up documents for you, with your permissions and in the audit trail under your name.

## Connecting Claude

1. In OpenSign, open **Settings → Account** and copy the server URL under **AI Apps (MCP)**. It is `https://<your-install>/mcp`.
2. In Claude, add a custom connector with that URL.
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

Each tool is a service the connected user's role allows, named with underscores (`documents.send` becomes `documents_send`). Results come back as JSON text plus `structuredContent`. Tools are annotated as read-only, destructive (delete, cancel, revoke, user changes) or open-world (calls that reach outside, such as sending or reminding). Files go in and out as base64.

| Area | Tools |
| --- | --- |
| Documents | `documents_list`, `documents_get`, `documents_create`, `documents_upload`, `documents_remove_file`, `documents_update`, `documents_set_parties`, `documents_set_fields`, `documents_check`, `documents_auth_methods`, `documents_send`, `documents_remind`, `documents_cancel`, `documents_delete`, `documents_share`, `documents_unshare`, `documents_signing_link`, `documents_download`, `documents_verify`, `documents_legal_hold`, `documents_move`, `documents_archive`, `documents_values`, `documents_export` |
| Folders | `folders_list`, `folders_create`, `folders_update`, `folders_delete` |
| Bulk send | `bulk_preview`, `bulk_start`, `bulk_list`, `bulk_get`, `bulk_cancel` |
| Webhooks | `webhooks_list`, `webhooks_create`, `webhooks_delete`, `webhooks_test`, `webhooks_deliveries`, `webhooks_delivery`, `webhooks_resend`, `webhooks_resend_failed` |
| Users | `users_list`, `users_invite`, `users_list_invites`, `users_revoke_invite`, `users_update` |
| Audit | `audit_list`, `audit_verify` |
| Account and keys | `account_me`, `api_keys_list`, `api_keys_update`, `api_keys_revoke` |
| Settings and system | `settings_get`, `settings_send_test_email`, `seal_get`, `retention_preview`, `system_version`, `system_status`, `ai_usage` |

Not offered as tools: signing in, passwords, two-factor and passkey setup, creating API keys, changing settings, the in-app update, replacing the seal key, in-person signing, anything a signer does, OAuth consent, and the AI chat itself.

## Things to try

- "Which documents have been waiting for signatures for more than a week? Remind those parties."
- "Create a draft from the NDA template for Anna Berg at Acme, anna@acme.example, with BankID to sign."
- "Has the Acme consulting agreement been signed? Download the sealed PDF."
- "Verify the hash chain of the audit log."
