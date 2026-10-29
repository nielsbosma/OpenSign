# Parties, roles and signing order

A party is anyone who takes part in a document: someone who signs, approves, or only receives the signed copy. You add parties on the document page under **Parties**.

## Party details

| Field | |
| --- | --- |
| Name | Required. Shown on the signing page and in the evidence. |
| Role | Signs, Approves or Receives a copy. See below. |
| Email | Needed for email delivery and to receive the signed copy. |
| Mobile | With country code, such as `+46 70 123 45 67`. Needed for SMS delivery and SMS codes. |
| Company | Optional. Fills Company fields. |
| Language | Their emails, SMS and signing page. **Same as Document** by default. See [The signing page](signing-page.md#languages). |
| Send Link By | How the party gets their signing link. See [Delivery](#delivery). |
| Identify to View | What the party must do before they can open the document. See [Identification](identification.md). |
| Identify to Sign | What the party must do before they can sign. See [Identification](identification.md). |
| Ask for Webcam Identification | The party takes a photo of themselves before signing. See [Webcam photo](identification.md#webcam-photo). |
| Personal Number | For eID identification. When set, only the eID of that person can sign. Stored encrypted and shown masked. |
| Signing Order | When the document signs in order, lower numbers go first. See [Signing order](#signing-order). |
| Ask for Attachments | Files the party must or may upload. See [Signer attachments](attachments.md). |

## Roles

| Role | What the party does |
| --- | --- |
| **Signs** (signer) | Reads the document, fills in their fields and signs with a drawn or typed signature. |
| **Approves** (approver) | Reads the document and approves it. No signature image, but the approval is in the evidence like a signature. |
| **Receives a copy** (viewer) | Does not act. Gets the sealed PDF when the document is completed. A viewer cannot have fields. |

The document is completed when every signer and approver has acted.

## Delivery

| Option | |
| --- | --- |
| **Email** | The default. OpenSign emails an invitation with the personal link, and reminders if they are on. |
| **SMS** | OpenSign texts the link. Needs an SMS provider in **Settings → SMS**. |
| **Link (you hand it over)** | OpenSign sends nothing. When you send the document, it shows you the link to pass on, for example in your own email or chat, or to show inside your own app with [embedded signing](embedding.md). |
| **In Person (on your device)** | The party signs on your computer or tablet, started from the document page. See [In-person signing](in-person.md). |
| **None** | OpenSign sends nothing and shows no link. Useful for viewers who only need the signed copy, which goes to their email address. |

Options that are not set up on this install are shown but cannot be chosen until an admin sets them up.

### Signing links

Each party has a personal link. It is only valid for that party, and it stops working when:

- the document is completed, declined, canceled or expires,
- the link's own lifetime runs out: the document's expiry date, or otherwise the number of days set in **Settings → Security** (30 by default), counted from when the party was first notified, or
- a new link is issued for the party, which revokes the old one.

The secret part of the link is after the `#`, so it never reaches server logs, and the signing page removes it from the address bar. See [Security model](security.md#signing-links).

## Signing order

By default everyone is invited at once and can sign in any order. Turn on **Sign in Order** on the **Parties** card to invite parties one step at a time:

- Each signer and approver shows its step number. Turning the order on starts from the order the parties are listed in.
- Use the arrows on a party card to move a party earlier or later.
- To let several parties sign together in the same step, give them the same **Signing Order** number under **Edit**. Lower numbers go first.
- When everyone in a step has acted, OpenSign invites the next step.
- A party who opens their link before it is their turn sees "Others sign before you" and is emailed when it is their turn.

Viewers are not part of the order. They get the sealed PDF at the end.

## Declining

A signer or approver can decline, with an optional reason. Declining stops the document for everyone: it becomes **Declined**, every link stops working, the sender is told, and a `document.declined` webhook fires with the reason.

## Changing parties

Parties can be changed while the document is a draft. Removing a party also removes their fields. Once a document is sent, its parties are fixed; to change them, cancel the document and start a new one.
