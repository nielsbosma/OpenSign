# Single sign-on

People can sign in to OpenSign through your organisation's identity provider, using OpenID Connect. It works with any provider that supports the authorization code flow and RS256-signed ID tokens, such as Microsoft Entra ID, Google, Okta and Wefix. Signers never use it: signing links and [identification](identification.md) work as before.

The sign-in page shows a button for it. Nobody is sent to the provider without clicking, so signing out stays signed out.

## Setting it up

1. At your provider, register a web application with this redirect URI, using your own `APP_URL`:

   ```text
   https://sign.example.com/api/auth/sso/callback
   ```

2. Make sure at least one admin exists. Single sign-on is refused until the first admin has been created with the [setup code](setup.md), and it never creates an admin.
3. In **Settings → Single Sign-On**, fill in the fields below and turn it on. Only an admin signed in to OpenSign can change these settings, in the browser or with an environment variable. The assistant, MCP and API keys cannot.

| Field | What it does |
| --- | --- |
| Button Text | The text on the sign-in button, such as "Sign In with Acme". Empty shows a general text. |
| Issuer URL | The provider's issuer. OpenSign reads `/.well-known/openid-configuration` under it, and the document must name the same issuer. It must be a public https address. |
| Client ID and Client Secret | From the application you registered. The secret is stored encrypted and is never shown again. |
| Role for New Accounts | The role of an account created at first sign-in. Viewer by default, and never Admin. |
| Require a Second Factor from the Provider | On by default. A sign-in is refused unless the ID token's `amr` claim contains `mfa`, `otp` or `hwk`. |
| Password Sign-In | **Everyone**, or **Admins Only** so that only admins use a password or passkey while single sign-on is on. |

OpenSign asks for the scopes `openid email profile`, and uses the claims `sub`, `email`, `email_verified`, `name` and `amr`.

## What happens at first sign-in

An account is identified by the provider's `iss` and `sub`, not by email. The email and name are updated from the token at each sign-in, and a changed email is recorded in the [audit log](audit-log.md).

| The first time someone arrives | Result |
| --- | --- |
| No account has their email | An account is created with **Role for New Accounts**. Change the role under **Settings → Users**. |
| A non-admin account has their email | It becomes a single sign-on account and keeps its documents. Its password, passkeys, authenticator app, API keys and connected apps are removed, and this is recorded in the audit log. |
| An admin account has their email | Refused. Admin accounts are never linked automatically, so nobody at the provider can take over an admin by giving themselves that address. |
| The provider has not verified the email | Refused. |

Roles never come from the provider. If you promote a single sign-on account to Admin, it keeps signing in through the provider, and everything at the provider then controls an admin account. Keep at least one admin with a password.

If you change **Issuer URL** to a different provider, existing single sign-on accounts are refused with a message about the email already being used, because their identity belongs to the old issuer.

## Single sign-on accounts

The provider owns these accounts, so OpenSign does not keep credentials for them:

- no password, passkeys, authenticator app or forgot-password,
- no API keys and no connecting [MCP](mcp.md) clients,
- the email cannot be changed in OpenSign. Change it at the provider.

Accounts show an **SSO** badge under **Settings → Users**.

## Two-factor

With **Require a Second Factor from the Provider** on, OpenSign only trusts what the ID token says about the sign-in, so the provider must send `amr`.

**Require two-factor authentication for everyone** in **Settings → Security** also applies: a single sign-on sign-in is refused without a second factor in `amr`, whatever this setting says. Turn that requirement off if your provider does not send `amr`, or that provider's users cannot sign in.

## Blocking someone

Blocking someone at the provider stops new sign-ins at once. A session that is already open keeps working until it expires: after 12 hours at most, or 60 minutes without use. OpenSign does not ask the provider again during a session. To stop someone right away, also **deactivate** them under **Settings → Users**, which signs them out everywhere.

## If the provider is down

With **Admins Only**, admins can still sign in with a password or passkey. Turning single sign-on off lets everyone with a password use it again, but accounts that were linked or created by single sign-on have no password and wait until it is back.

## Microsoft Entra ID

1. In **App registrations**, register an application with the redirect URI above, of type **Web**, and create a client secret.
2. Use `https://login.microsoftonline.com/<tenant id>/v2.0` as **Issuer URL**. A single tenant is required, because the `common` address does not name one issuer.
3. Entra ID does not send `email_verified`. Under **Token configuration**, add the optional ID token claims `email` and `xms_edov`, or every sign-in is refused because the email is not verified. OpenSign accepts `xms_edov` in place of `email_verified`.
4. Entra ID's v2 tokens may not contain `amr`. Turn **Require a Second Factor from the Provider** off if yours do not, and require multi-factor authentication with Conditional Access for the application instead.

See Microsoft's [optional claims reference](https://learn.microsoft.com/en-us/entra/identity-platform/optional-claims-reference).

## Google

1. In the Google Cloud console, under **APIs & Services → Credentials**, create an **OAuth client ID** of type **Web application** with the redirect URI above.
2. Use `https://accounts.google.com` as **Issuer URL**.
3. Google does not send `amr`, so turn **Require a Second Factor from the Provider** off and require 2-Step Verification in your Google Workspace admin console.
4. Set the OAuth consent screen to **Internal**. Otherwise anyone with a Google account can sign in and get an account with **Role for New Accounts**.

## Okta

Create an **OIDC Web Application** with the redirect URI above, and use your authorization server as **Issuer URL**, such as `https://acme.okta.com`. Only people assigned to the application can sign in.

## Wefix

Register OpenSign as a client in Wefix with the redirect URI above, then use:

| Field | Value |
| --- | --- |
| Button Text | Sign In with Wefix |
| Issuer URL | Your Wefix address |
| Client ID and Client Secret | From the Wefix client |
| Role for New Accounts | Sender |
| Require a Second Factor from the Provider | On |
| Password Sign-In | Admins Only |

## When a sign-in is refused

The person sees why on the sign-in page, and the attempt is in the [audit log](audit-log.md) as `auth.sso_refused` with the reason. The provider's own details stay in the server log.

| Reason in the log | What to check |
| --- | --- |
| `email_unverified` | The provider did not send `email_verified` as true. On Entra ID, add `xms_edov`. |
| `mfa_required` | The ID token has no `mfa`, `otp` or `hwk` in `amr`. |
| `admin_account` | The email belongs to an admin. That person signs in with their password. |
| `email_conflict` | The email is on an account with another identity, often after changing issuer. |
| `inactive` | The account is deactivated. |
| `denied` | The provider answered with an error, such as the person not being assigned to the application. |
| `failed` | Discovery, the token request or the ID token check failed. The server log has the cause. |
| `other_browser` | The sign-in finished in a different browser than it started in. |

## For development

Only with `APP_ENV=development` may **Issuer URL** be `http` or a private address, so you can test against a local provider. Never set that on a real install.
