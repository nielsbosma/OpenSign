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

The party fills in their fields: text, numbers, dropdowns, multi-select lists, radio groups, checkboxes, phone numbers, dates from a date picker, images and files they upload in place, initials and signatures. Each field is checked as they type, with errors in their language or the [error text you wrote](documents.md#checks-and-error-texts), and they cannot sign until every field passes. Name, email and company fields are filled in from their details. If you asked for [attachments](attachments.md), they upload them here. If you asked for a [webcam photo](identification.md#webcam-photo), they take it here.

In a document that signs in order, a party who opens their link early sees "Others sign before you. We will email you when it is your turn."

## Signing

A signature is either:

- **Draw**: with a finger, a pen or the mouse, or
- **Type**: the party types their name, and it is shown in a signature style, or
- **Saved**: a signature saved earlier, offered first when there is one. OpenSign users who sign their own documents get the one from their [account](account-security.md#saved-signature). Other signers can tick **Remember my signature in this browser** when they sign; next time a party with the same name signs in that browser, it is offered here, with **Forget** to remove it. It stays in the browser and is never sent anywhere until they sign with it. Nothing is remembered during in-person signing on someone else's device.

Before signing, the party confirms: "I have read the document and agree to sign it electronically. This is as binding as a signature on paper." If the party must identify to sign, they do that now. Then they choose **Sign** (or **Approve** for approvers).

The party sees a confirmation. When everyone has signed, parties with an email address get the sealed PDF by email, and the signing page offers **Download Signed Document** while the link is still valid.

## Declining

**Decline** asks for an optional reason and stops the document for everyone. The sender is told, along with every party who already had a link.

## Languages

Signers can be addressed in eleven languages: English, Chinese (Simplified), Hindi, Spanish, French, Arabic, Bengali, Portuguese, Russian, Urdu and Swedish. Arabic and Urdu are shown right to left.

Each party's language decides their invitation and reminder emails, SMS texts and signing page. It is the **Language** on the party if you set one, otherwise the document's **Language for Signers**, otherwise the default from **Settings → Organisation**.

The signing page has a language picker in its header. A signer's choice lasts only for that browser tab, so it does not carry over to the next person on a shared device.

Some texts are still in English or Swedish: BankID's own prompts, error messages from the server, and the evidence pages in the sealed PDF.

## Review with your AI

Signers can ask their own AI assistant, such as ChatGPT, Claude or Copilot, to explain the document before they sign. Under **Review with Your AI** they choose **Create Review Link**, then copy a ready-made prompt or choose **Open in ChatGPT**, **Claude** or **Copilot**, which opens the assistant with the prompt filled in.

The review link is not the signing link:

- It is read-only. It gives the assistant the document's text and the PDF as sent, and cannot sign, decline or change anything.
- It is shown once, and needs the same identification as viewing the document.
- It expires after the days set in **Settings → Security** (7 by default), or earlier at the document's deadline or the signing link's expiry, and allows at most 100 fetches.
- It stops working when the signer revokes it, when you issue a new signing link, or when the document is no longer pending.
- Every use is in the audit trail.

The text is marked for the assistant as untrusted, and pages with hidden text, such as white or invisible text, are pointed out so the assistant can warn the signer about them.

You can turn this off per document under **AI Review by Signers** in the document editor, which also revokes links already shared. An admin can turn it off for everyone by setting the days to 0.

## Previewing as a party

Choose **Preview** in the document editor to see the signing page as a chosen party will see it: your branding, the message, their language, their fields, attachment requests and identification steps. Nothing can be signed or sent from the preview, and it is not recorded as the party opening the document.

## Branding

The page shows your organisation's name and logo and uses your brand color, all from **Settings → Organisation**. See [Logo](settings.md#logo).

The page can also be shown inside your own app. See [Embedded signing](embedding.md).

## Privacy and safety

- The secret part of the link is after the `#`, so it is not sent to the server in requests or logs, and the page removes it from the address bar once loaded.
- Signer actions never use a signed-in OpenSign session, even if the signer happens to have one in the same browser.
- Every step is recorded in a tamper-evident audit trail, as the page tells the signer.
