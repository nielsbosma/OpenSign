# The signing page

This is what the people you send documents to see. They need no account and no app, only a browser. The page works on phones, tablets and computers, in light and dark mode.

## Opening the link

The invitation comes by email or SMS, or from you directly. It names your organisation and the person who sent the document, and says until when the link works.

If the party must identify to view the document, they do that first: with an SMS code, BankID, Freja or another eID. See [Identification](identification.md). Otherwise the document opens straight away.

A link that is incomplete, revoked, expired, or belongs to a document that is no longer pending shows a clear message instead of the document.

## Reading and filling in

The page shows:

- who the document is from, and the message to parties if there is one,
- what the party is asked to do: sign, approve, or receive a copy,
- the whole document, with the party's own fields highlighted.

The party fills in their fields: text, dates, checkboxes, initials and signatures. Name, email and company fields are filled in from their details. If you asked for [attachments](attachments.md), they upload them here.

In a document that signs in order, a party who opens their link early sees "Others sign before you. We will email you when it is your turn."

## Signing

A signature is either:

- **Draw**: with a finger, a pen or the mouse, or
- **Type**: the party types their name, and it is shown in a signature style.

Before signing, the party confirms: "I have read the document and agree to sign it electronically. This is as binding as a signature on paper." If the party must identify to sign, they do that now. Then they choose **Sign** (or **Approve** for approvers).

The party sees a confirmation. When everyone has signed, parties with an email address get the sealed PDF by email, and the signing page offers **Download Signed Document** while the link is still valid.

## Declining

**Decline** asks for an optional reason and stops the document for everyone. The sender is told, along with every party who already had a link.

## Languages

Everything a signer reads follows the document's **Language for Signers**: the invitation and reminder emails, the SMS texts, the signing page and the BankID messages. The default comes from **Settings → Organisation**, and each document can use its own.

Signers can currently be addressed in **English** and **Swedish**.

> **Coming soon.** Eleven signer languages (English, Mandarin Chinese, Hindi, Spanish, French, Arabic, Bengali, Portuguese, Russian, Urdu and Swedish), a language per party that overrides the document's, and a language picker on the signing page. Arabic and Urdu are shown right to left.

## Branding

The page shows your organisation's name and uses your brand color from **Settings → Organisation**.

## Privacy and safety

- The secret part of the link is after the `#`, so it is not sent to the server in requests or logs, and the page removes it from the address bar once loaded.
- Signer actions never use a signed-in OpenSign session, even if the signer happens to have one in the same browser.
- Every step is recorded in a tamper-evident audit trail, as the page tells the signer.
