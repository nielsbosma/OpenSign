# First-run setup and email

A fresh install walks you through two steps before anyone can use it: create the first admin, then connect email. Until both are done, OpenSign sends every admin back to the setup page.

## Step 1: the first admin

When OpenSign starts and has no users, it prints a one-time **setup code** in its log:

```
No admin yet. Open https://sign.example.com and enter the setup code …
```

Open the install in a browser and fill in:

| Field | |
| --- | --- |
| Setup Code | The code from the log. It proves you have access to the server, so nobody who finds a fresh install on the internet can claim it. |
| Organisation | Your organisation's name. Signers see it in emails and on the signing page. You can change it later in **Settings → Organisation**. |
| Your Name, Email, Password | Your admin account. |

Installs from the **Deploy to Sliplane** button skip the code: the deployer generates a `SETUP_CODE`, and **Open OpenSign** on its finish page goes to `/setup#code=…`, which fills it in. The code sits in the URL fragment, which the browser never sends to a server, and the page removes it from the address bar. If you open the install another way, the code is shown on the deployer's finish page and in the service's environment in Sliplane. With `SETUP_CODE` set, the log no longer prints a code.

Once the first admin exists, the setup code stops working for good. Everyone else is invited from **Settings → Users** (see [Users and roles](users-and-roles.md)).

## Step 2: email

OpenSign sends signing invitations, reminders, notices when a document is stopped, the sealed documents, user invitations and password resets by email. Setup asks for a working email provider, Resend or your own SMTP server, and sends you a test message. Setup only finishes once the provider accepts it, and documents cannot go out by email before that.

### Resend

[Resend](https://resend.com) is the default provider.

1. Create a Resend account and verify the domain you want to send from under **Domains**.
2. Create an API key under **API Keys**, with permission to send.
3. In OpenSign, enter the API key, a **From Address** on the verified domain (such as `sign@example.com`) and a **From Name**.

### SMTP

Choose the **SMTP Server** tab during setup, or **SMTP** as the **Provider** in **Settings → Email**, to send through your own mail server or another provider's SMTP relay.

| Field | |
| --- | --- |
| SMTP Server | The host name, such as `smtp.example.com`. No `smtp://` and no port. |
| Port | Usually 587 for STARTTLS or 465 for TLS. |
| Security | **STARTTLS** or **TLS**. |
| Username, Password | Empty username sends without signing in. The password is stored encrypted. |
| From Address, From Name | The sender. |

TLS is always required and the certificate must be valid for the server name, so credentials never travel unencrypted. There is no plain-text option. Servers on private networks are refused, except in development.

### Changing email later

Email settings live in **Settings → Email**. Changing the provider, key, SMTP server or sender clears the "verified" state, and OpenSign asks for a new test email before documents can go out again. **Reply-To** is optional: empty sends replies to the person who sent each document.

## What to set up next

All other configuration is done in the browser, under **Settings**. None of it is needed to start sending documents with email links.

- **Organisation**: name, registration number, country, time zone, default signer language and brand color. See [Admin settings](settings.md).
- **SMS**: for SMS invitations and one-time codes. See [Identification](identification.md#sms-codes).
- **BankID and eID**: Swedish BankID, Freja eID and other Nordic eIDs. See [Identification](identification.md).
- **Signing and Seal**: the seal name, reason and time stamp authority. See [Sealed PDFs and evidence](sealed-pdfs.md).
- **Security**: require two-factor sign-in for everyone. See [Sign-in, passkeys and two-factor](account-security.md).
- **AI**: optional. See [AI assistant and review](ai.md).

## Pinning settings with environment variables

Any admin setting can be set with an environment variable named `OPENSIGN_<GROUP>_<FIELD>`, for example `OPENSIGN_EMAIL_API_KEY` or `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR=true`. A pinned field is locked in the browser and shows "Set by an environment variable." This is handy for infrastructure as code, and for keeping a secret out of the database entirely. See [Admin settings](settings.md#pinning-settings) for the full list of names.
