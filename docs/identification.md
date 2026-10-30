# Identification

For each party you choose how strongly they must prove who they are, separately for opening the document (**Identify to View**) and for signing it (**Identify to Sign**). Whatever they use is recorded in the audit trail and on the evidence pages of the sealed PDF.

## Methods

| Method | To view | To sign | Needs |
| --- | --- | --- | --- |
| **None** | Default | | Holding the link is enough to open the document. |
| **Standard** | | Default | Holding the link. Typical for email and SMS delivery, since only the recipient gets the link. |
| **SMS code** (`sms_pin`) | Yes | Yes | An SMS provider, and a mobile number on the party. |
| **Swedish BankID** (`se_bankid`) | Yes | Yes | BankID directly or through TIC Identity. |
| **Freja eID** (`freja`) | Yes | Yes | A Freja eID agreement. |
| **Other eID** (`broker`) | Yes | Yes | An OpenID Connect eID broker, such as Idura (Criipto) or Signicat. |

A method that is not set up on the install is shown disabled in the party dialog. Sending is refused if a party uses a method that is not ready, so a signer never gets stuck.

## Webcam photo

Tick **Ask for Webcam Identification** on a signer or approver. Before they can sign, they take a live photo of themselves with their computer's camera or their phone: **Turn On Camera**, **Take Photo**, then **Use Photo** or **Retake**. There is no file picker.

- The photo is cleaned of EXIF and location data, stored encrypted, and embedded in the sealed PDF with its SHA-256 hash. The audit trail records when it was taken.
- On the document page, the party card shows the photo and when it was taken. Every view is recorded in the audit trail.
- Only the signing page may use the camera. Every other page in OpenSign blocks it.

A photo is evidence of who signed, not proof of liveness: a skilled signer could send a still image instead of a live one. For strong identification, use an eID.

## Personal numbers

For BankID and Freja, put the party's **Personal Number** on the party. OpenSign then requires that the eID names exactly that person: an identification by anyone else is refused and recorded in the audit trail. Without a personal number, anyone who holds the link and has an eID could sign, and the setup check warns about it.

The personal number is stored encrypted and shown masked (only the last four digits). When editing a party, leave the field empty to keep the stored number.

## SMS codes

The party asks for a code on the signing page and gets a 6-digit code by SMS.

- A code is valid for 10 minutes and allows 5 attempts.
- At most 5 codes per hour can be sent to a party.
- Codes are stored only as a keyed hash.

### Setting up SMS

In **Settings → SMS**, pick a provider:

| Provider | Username | Password |
| --- | --- | --- |
| [46elks](https://46elks.com) | API username | API password |
| [Twilio](https://www.twilio.com) | Account SID | Auth token |

A Twilio API key works too, and can be revoked on its own: the Account SID stays in **Username**, the key goes in **API Key SID** and its secret in **Password**. Only keys made in Twilio's US1 region work.

**Sender** is a name of up to 11 letters, or a number you own. With Twilio it can also be a Messaging Service SID (`MG…`). The same provider is used for SMS delivery of signing links.

## Swedish BankID

The signer identifies with the BankID app: on the same device with **Open BankID on This Device**, or by scanning a QR code with **BankID on Another Device**. If the party has a personal number, only that person's BankID can answer.

There are two ways to connect BankID. Parties choose "Swedish BankID" either way.

### With your own BankID agreement

1. In **Settings → BankID and eID**, turn on **Swedish BankID**.
2. To try it, turn on **Use the BankID test environment** and leave the certificate empty. OpenSign uses BankID's public test certificate, and you can sign with a [test BankID](https://developers.bankid.com/test-portal).
3. For production, turn the test environment off and upload the relying-party certificate your bank gave you (a `.p12` file) with its password.

OpenSign talks to BankID over mutual TLS and trusts only BankID's own root certificate, which is built in. Replace it under **BankID Server CA** only if BankID tells you to.

### Through TIC Identity

[TIC Identity](https://id.tic.io) offers BankID without your own agreement with a bank.

1. In TIC's portal, turn on OpenID Connect for your tenant and register `https://<your-install>/api/eid/callback` as a redirect URI.
2. In **Settings → BankID and eID**, turn on **BankID Through TIC Identity** and enter the tenant name (as in `https://id.tic.io/<tenant>`), client id and client secret.

TIC is used whenever BankID with your own certificate is off. OpenSign only trusts the Swedish personal number in TIC's `nin` claim, and requires a valid one.

## Freja eID

1. In **Settings → BankID and eID**, turn on **Freja eID**. Use **Use the Freja test environment** while testing.
2. Upload the **Client Certificate** from Freja (a `.pfx` file) with its password.
3. Add the **Freja Result-Signing Certificate**. OpenSign refuses any result whose signature does not check out against it.

The signer confirms in the Freja app.

## Other Nordic eIDs through a broker

An OpenID Connect broker such as Idura (Criipto) or Signicat gives access to Norwegian and Danish BankID, MitID, Finnish bank IDs and more.

1. Create an application at the broker and register `https://<your-install>/api/eid/callback` as the redirect URI.
2. In **Settings → BankID and eID**, turn on **eID broker (OpenID Connect)** and enter the **issuer URL**, **client id** and **client secret**.
3. Optionally set **eID Choice (acr_values)** to pick one eID, such as `urn:grn:authn:no:bankid`. Empty lets the signer choose at the broker.

The signer is sent to the broker and back. OpenSign uses the authorization code flow with PKCE, state and nonce, and checks the ID token's signature, issuer, audience and expiry.

## What is recorded

For each identification, OpenSign keeps the method, the name and (masked) personal number the eID returned, the time, the IP address and the browser. eID evidence is stored encrypted. It appears in the audit trail and on the evidence pages of the sealed PDF. See [Sealed PDFs and evidence](sealed-pdfs.md).
