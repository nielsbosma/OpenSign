# Sign-in, passkeys and two-factor

Your account is protected by a password and, ideally, a second factor: an authenticator app or a passkey. Manage both under **Settings → Account**.

## Passwords

- At least 12 characters. Long passphrases are welcome.
- Passwords are hashed with Argon2id. Nobody, including admins, can read them.
- After repeated failed sign-ins the account is locked for a while, starting at one minute and growing to an hour. Every attempt is recorded.
- **Change Password** asks for the current one and signs out your other sessions.
- **Forgot Your Password?** on the sign-in page emails a reset link that works for an hour.

## Two-factor with an authenticator app

1. Under **Two-Factor Authentication**, start the setup and scan the QR code with an authenticator app, such as 1Password, Google Authenticator or Microsoft Authenticator.
2. Enter the 6-digit code to confirm.
3. Save the **recovery codes**. Each signs you in once if you lose your phone. They are shown only this once.

From then on, signing in asks for a code after your password. If you lose both your phone and your recovery codes, an admin can reset your two-factor (see [Users and roles](users-and-roles.md)).

## Passkeys

A passkey lets you sign in with Face ID, Touch ID, Windows Hello, your phone or a security key, without typing a password.

1. Under **Passkeys**, choose **Add a Passkey**, give it a name (such as "MacBook") and confirm your password.
2. Follow your browser's prompt.

Then choose **Sign In with a Passkey** on the sign-in page. A passkey always verifies you on the device (with a fingerprint, face or PIN), so it counts as two-factor on its own and no code is asked for. You can add several and remove any of them.

## Requiring two-factor for everyone

An admin can turn on **Require two-factor authentication for everyone** in **Settings → Security**. Anyone without an authenticator app or a passkey is then asked to set one up before they can do anything else. MCP clients signed in as such an account are refused until they do. We recommend turning this on.

## Language

Under **Settings → Account and Security**, pick the language OpenSign is shown in for you: English, Chinese (Simplified), Hindi, Spanish, French, Arabic, Bengali, Portuguese, Russian, Urdu or Swedish. Before you sign in, the app follows your browser, and the sign-in page has a picker. This does not change what signers see; they get the language set on their document or party. Some messages from the server are still in English.

## Saved signature

Under **Settings → Account and Security → My Signature**, draw or type your signature once, and add your initials if you like. When you sign a document yourself, the signing page offers it on a **Saved** tab. It is never applied by itself: you still choose to sign, and you can draw or type a new one instead.

It is offered only when the signing link belongs to you: when you [sign a document yourself](in-person.md#signing-a-document-yourself), or open a link sent to your own email address while signed in. Someone you hand your device to for in-person signing never sees it. The drawing is stored encrypted like every other file, and **Remove** deletes it. Saving and removing it are recorded in the audit trail.

Outside signers have no account, so they can tick **Remember my signature in this browser** instead. That keeps it in their browser only, offered next time to a party with the same name; nothing about them is stored on the server between documents. See [The signing page](signing-page.md#signing).

## Sessions

Sessions are browser cookies that scripts cannot read, bound to your install, and stored on the server only as a keyed hash. In **Settings → Security** an admin sets:

| Setting | Default | |
| --- | --- | --- |
| Session Length (Hours) | 12 | How long a sign-in lasts at most. |
| Sign Out After Idle (Minutes) | 60 | How long a session survives without use. |

**Sign Out** in the user menu ends the session in this browser.

## Apps connected through MCP

Under **AI Apps (MCP)** you see the MCP server address for your install and the clients you have connected, such as Claude. Disconnect any of them at any time. See [MCP and Claude](mcp.md).

## API keys

API keys are managed under **Settings → API Keys**. See [REST API](api.md#api-keys).
