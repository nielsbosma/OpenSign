# Admin settings

Almost everything about an install is configured in the browser, under **Settings**, by an admin (the **settings.manage** permission). Only a handful of environment variables are needed to start the server; see [Deployment](deployment.md#environment-variables).

Every change is recorded in the audit trail, with secrets masked. Secret values (API keys, passwords, certificates, client secrets, the deploy hook) are stored encrypted, are never shown again after saving, and show only whether they are set. To keep a stored secret, leave its field empty when saving.

## Organisation

Shown to the people you send documents to, in emails and on the signing page.

| Field | Variable | Default |
| --- | --- | --- |
| Name | `OPENSIGN_ORG_NAME` | Set during setup |
| Registration Number | `OPENSIGN_ORG_REGISTRATION_NUMBER` | |
| Country | `OPENSIGN_ORG_COUNTRY` | `SE` |
| Time Zone | `OPENSIGN_ORG_TIME_ZONE` | `Europe/Stockholm` |
| Language for Signers | `OPENSIGN_ORG_LOCALE` | `en`. The default for new documents; see [The signing page](signing-page.md#languages). |
| Brand Color | `OPENSIGN_ORG_BRAND_COLOR` | `#4f46e5`. Used for buttons on the signing page and in emails, and for the OpenSign mark when there is no logo. |

### Logo

Upload your organisation's logo under **Settings → Organisation → Logo**. It replaces the OpenSign mark in:

- the app's sidebar and the sign-in, setup and [verification](verification.md) pages,
- the [signing page](signing-page.md#branding) and its preview,
- every email OpenSign sends,
- the evidence page at the end of each [sealed PDF](sealed-pdfs.md).

Without a logo, the OpenSign mark is shown in your brand color.

The logo must be a PNG or JPEG of at most 2 MB, between 16 and 2000 pixels on each side. A PNG with a transparent background looks best on both light and dark pages. SVG is not accepted, because an SVG file can contain script; export it as PNG first.

What happens to an upload:

- The image is decoded and encoded again as a new PNG. This keeps transparency but drops metadata (such as camera details or GPS positions) and anything else hidden in the file.
- It is stored encrypted in your S3 bucket, like documents.
- It is served at `/api/logo/<id>.png` without signing in, because signers and email clients load it. Only the current logo is served there; a replaced or removed logo is deleted. Each new logo gets a new address, so nobody sees an old one from a cache.
- Uploading and removing are recorded in the audit trail as `branding.logo_set` and `branding.logo_removed`.

Through the API, use `branding.set_logo` (with the image as base64) and `branding.remove_logo`. `branding.get` returns the name, brand color and logo address and needs no sign-in.

## Email

See [First-run setup and email](setup.md#step-2-email).

| Field | Variable | |
| --- | --- | --- |
| Provider | `OPENSIGN_EMAIL_PROVIDER` | `resend` (default) or `smtp`. |
| API Key | `OPENSIGN_EMAIL_API_KEY` | Secret. The Resend API key. |
| SMTP Server | `OPENSIGN_EMAIL_SMTP_HOST` | For SMTP. A host name, no scheme or port. |
| Port | `OPENSIGN_EMAIL_SMTP_PORT` | For SMTP. 587 by default. |
| Security | `OPENSIGN_EMAIL_SMTP_SECURITY` | For SMTP. `starttls` (default) or `tls`. |
| Username | `OPENSIGN_EMAIL_SMTP_USERNAME` | For SMTP. Empty: send without signing in. |
| Password | `OPENSIGN_EMAIL_SMTP_PASSWORD` | For SMTP. Secret. |
| From Address | `OPENSIGN_EMAIL_FROM_ADDRESS` | On a domain you verified with the provider. |
| From Name | `OPENSIGN_EMAIL_FROM_NAME` | |
| Reply-To | `OPENSIGN_EMAIL_REPLY_TO` | Empty: replies go to the sender of each document. |

Changing the provider, key, SMTP server or sender needs a new test email before documents can go out.

## Security

See [Sign-in, passkeys and two-factor](account-security.md).

| Field | Variable | Default |
| --- | --- | --- |
| Require two-factor authentication for everyone | `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR` | Off |
| Session Length (Hours) | `OPENSIGN_SECURITY_SESSION_HOURS` | 12 |
| Sign Out After Idle (Minutes) | `OPENSIGN_SECURITY_IDLE_MINUTES` | 60 |
| Signing Links Expire After (Days) | `OPENSIGN_SECURITY_SIGNER_LINK_DAYS` | 30. Used when a document has no expiry date. |
| Largest Upload (MB) | `OPENSIGN_SECURITY_MAX_UPLOAD_MB` | 25 |
| AI Review Links Expire After (Days) | `OPENSIGN_SECURITY_REVIEW_LINK_DAYS` | 7. 0 turns signers' AI review links off; at most 30. See [The signing page](signing-page.md#review-with-your-ai). |
| Allow Embedded Signing | `OPENSIGN_SECURITY_EMBED_SIGNING` | Off. Lets the sites below show the signing page in a frame. See [Embedded signing](embedding.md). |
| Sites Allowed to Embed | `OPENSIGN_SECURITY_EMBED_ORIGINS` | None. Exact https origins, one per line; in the variable, separated by spaces or commas. |

## Updates

On the **System** page, below the version and background jobs. See [Updates](updates.md).

| Field | Variable | Default |
| --- | --- | --- |
| Check for Updates | `OPENSIGN_UPDATES_CHECK_FOR_UPDATES` | On |
| Deploy Hook | `OPENSIGN_UPDATES_DEPLOY_HOOK_URL` | Secret. Empty. |
| Update Link | `OPENSIGN_UPDATES_UPDATE_URL` | Empty. Set by the Sliplane Deployer. |

## Retention

See [Retention and legal hold](retention.md). 0 keeps documents forever.

| Field | Variable | Default |
| --- | --- | --- |
| Delete Signed Documents After (Days) | `OPENSIGN_RETENTION_COMPLETED_DAYS` | 0 |
| Delete Declined, Canceled and Expired Documents After (Days) | `OPENSIGN_RETENTION_ENDED_DAYS` | 0 |
| Delete Untouched Drafts After (Days) | `OPENSIGN_RETENTION_DRAFT_DAYS` | 0 |

## Signing and Seal

See [Sealed PDFs and evidence](sealed-pdfs.md).

| Field | Variable | Default |
| --- | --- | --- |
| Seal Name | `OPENSIGN_SIGNING_SEAL_NAME` | Empty: the organisation name |
| Signature Reason | `OPENSIGN_SIGNING_REASON` | `Signed with OpenSign` |
| Timestamp Authority (RFC 3161) | `OPENSIGN_SIGNING_TSA_URL` | `http://timestamp.digicert.com` |

The same page shows the **Seal Certificate** and lets you replace the seal key.

## AI

See [AI assistant and review](ai.md).

| Field | Variable | |
| --- | --- | --- |
| Provider | `OPENSIGN_AI_PROVIDER` | Off, `anthropic`, `openai`, `berget`, `opper`, `litellm` or `openai-compatible` |
| Model | `OPENSIGN_AI_MODEL` | Such as `claude-sonnet-5-5` |
| API Key | `OPENSIGN_AI_API_KEY` | Secret |
| Base URL | `OPENSIGN_AI_BASE_URL` | Only for LiteLLM and other compatible servers |
| Allow document text to be sent for reviews | `OPENSIGN_AI_ALLOW_DOCUMENT_TEXT` | Off |
| Monthly Token Budget | `OPENSIGN_AI_MONTHLY_TOKEN_BUDGET` | 0 means no limit |

## SMS

See [Identification](identification.md#sms-codes).

| Field | Variable | |
| --- | --- | --- |
| Provider | `OPENSIGN_SMS_PROVIDER` | Off, `46elks` or `twilio` |
| Username or Account SID | `OPENSIGN_SMS_USERNAME` | |
| Password or Auth Token | `OPENSIGN_SMS_PASSWORD` | Secret |
| Sender | `OPENSIGN_SMS_SENDER` | A name of up to 11 letters, or a number you own |

## BankID and eID

See [Identification](identification.md).

| Field | Variable | |
| --- | --- | --- |
| Swedish BankID | `OPENSIGN_EID_BANKID_ENABLED` | |
| Use the BankID test environment | `OPENSIGN_EID_BANKID_TEST` | |
| Relying-Party Certificate | `OPENSIGN_EID_BANKID_CERTIFICATE` | Secret. In the browser, upload the `.p12` with its password; as a variable, a PEM with the private key. |
| BankID Server CA | `OPENSIGN_EID_BANKID_CA_CERTIFICATE` | Optional. BankID's root is built in. |
| Freja eID | `OPENSIGN_EID_FREJA_ENABLED` | |
| Use the Freja test environment | `OPENSIGN_EID_FREJA_TEST` | |
| Client Certificate | `OPENSIGN_EID_FREJA_CERTIFICATE` | Secret. A `.pfx` with its password, or a PEM with the private key. |
| Freja Result-Signing Certificate | `OPENSIGN_EID_FREJA_SIGNING_CERTIFICATE` | PEM |
| Freja Server CA | `OPENSIGN_EID_FREJA_CA_CERTIFICATE` | Optional |
| eID broker (OpenID Connect) | `OPENSIGN_EID_BROKER_ENABLED` | |
| Issuer URL | `OPENSIGN_EID_BROKER_ISSUER` | |
| Client ID | `OPENSIGN_EID_BROKER_CLIENT_ID` | |
| Client Secret | `OPENSIGN_EID_BROKER_CLIENT_SECRET` | Secret |
| eID Choice (acr_values) | `OPENSIGN_EID_BROKER_ACR_VALUES` | Empty lets the signer choose |
| BankID Through TIC Identity | `OPENSIGN_EID_TIC_ENABLED` | |
| Tenant | `OPENSIGN_EID_TIC_TENANT` | |
| Client ID | `OPENSIGN_EID_TIC_CLIENT_ID` | |
| Client Secret | `OPENSIGN_EID_TIC_CLIENT_SECRET` | Secret |

## Pinning settings

Any field above can be pinned with its environment variable. The variable is named `OPENSIGN_<GROUP>_<FIELD>`, with the field name in upper snake case, as listed in the tables. A pinned field:

- always has the variable's value, whatever is stored,
- is locked in the browser, with the hint "Set by an environment variable.",
- if it is a secret, never touches the database at all.

Booleans are `true` or `false`; numbers are plain numbers. Pinning is a good fit for infrastructure as code, and the Sliplane template uses it to hand over the Resend key and AI provider.
