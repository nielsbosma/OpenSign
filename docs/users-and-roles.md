# Users and roles

Everyone who sends documents or administers OpenSign has an account. Signers do not need one. An install holds one organisation, and every account belongs to it.

## Inviting people

In **Settings → Users**, choose **Invite** and enter the person's email address and role. They get an email with a link to choose their own password. The link works for 7 days.

Pending invitations are listed on the same page and can be revoked, which stops the link working at once. Inviting and changing users needs the **users.manage** permission.

## Roles

| Role | For | Can |
| --- | --- | --- |
| **Admin** | People who run the install | Everything, including users, settings and the seal certificate. |
| **Manager** | Team leads, legal | Send documents, see and manage every document, manage shared templates, place legal holds, add webhooks. |
| **Sender** | Most people | Send documents, and see their own documents and those shared with them. |
| **Auditor** | Compliance, internal audit | Read every document and the audit log, and see system status. Changes nothing. |
| **Viewer** | Occasional readers | See documents shared with them. |

### Permissions

Roles are sets of permissions. Every check happens on the server, whichever way a request arrives (the web app, the REST API, Scrive or DocuSign endpoints, MCP or the AI assistant). The app only hides what the server would refuse anyway.

| Permission | Allows | Admin | Manager | Sender | Auditor | Viewer |
| --- | --- | :-: | :-: | :-: | :-: | :-: |
| `org.read` | See organisation details and the dashboard | ✓ | ✓ | ✓ | ✓ | ✓ |
| `users.read` | See who has an account | ✓ | ✓ | ✓ | ✓ | ✓ |
| `documents.read` | See your own documents and those shared with you | ✓ | ✓ | ✓ | ✓ | ✓ |
| `documents.create` | Upload documents and send them for signing | ✓ | ✓ | ✓ | | |
| `api_keys.manage` | Create API keys that act with your permissions | ✓ | ✓ | ✓ | | |
| `ai.use` | Use the AI assistant and document review | ✓ | ✓ | ✓ | | |
| `documents.read_all` | See every document | ✓ | ✓ | | ✓ | |
| `documents.manage_all` | Change, cancel and delete any document | ✓ | ✓ | | | |
| `documents.export` | Export filled-in values as CSV, Excel or JSON. See [Exporting filled-in data](data-export.md) | ✓ | ✓ | ✓ | ✓ | |
| `documents.legal_hold` | Place and release legal holds | ✓ | ✓ | | | |
| `templates.manage` | Create and change shared templates | ✓ | ✓ | | | |
| `webhooks.manage` | Add and remove webhooks | ✓ | ✓ | | | |
| `audit.read` | Browse and verify the audit log | ✓ | | | ✓ | |
| `system.read` | See version, migrations and background jobs | ✓ | | | ✓ | |
| `users.manage` | Invite people, change roles and deactivate accounts | ✓ | | | | |
| `settings.manage` | Change admin settings | ✓ | | | | |

## Sharing documents

A document belongs to the person who created it. To let a colleague work on it, share it from the document page: they can then see it, and optionally change it. Unsharing takes access away again. People with `documents.read_all` see every document without sharing.

## Changing and removing accounts

In **Settings → Users** an admin can:

- **edit details** to change someone's name or email. A new email is not used until someone opens the confirmation link sent to it within 24 hours; the old address then gets a notice that it changed,
- change someone's role. A role change signs them out everywhere, and also narrows or widens what their API keys can do, since a key never has more than its owner's role (see [REST API](api.md#api-keys)),
- **deactivate** an account, which signs the person out everywhere and revokes their API keys; **reactivate** it later if needed,
- **reset two-factor** for someone who lost their phone. If two-factor is required, they set it up again at their next sign-in.

Accounts are deactivated rather than deleted, so the audit trail always knows who did what. You cannot change your own role or deactivate yourself; ask another admin. Users can change their own password, two-factor and passkeys under **Settings → Account**. Admins can also edit their own name and email there; everyone else asks an admin.
