# Documents

A document in OpenSign is one or more files to sign, the parties who take part, and the fields each party fills in. You prepare it as a draft, send it, and OpenSign takes it from there until it is signed, declined, canceled or expired.

## The life of a document

| Status | What it means |
| --- | --- |
| **Draft** | You are still preparing it. Files, parties and fields can change. |
| **Pending** | Sent and waiting for signatures. Its files are locked into one PDF that can no longer change. |
| **Completed** | Everyone who had to act has signed or approved. The sealed PDF is ready. |
| **Declined** | A party declined. Signing stopped for everyone. |
| **Canceled** | The sender (or a manager) stopped it. |
| **Expired** | The deadline passed before everyone had signed. |

A sent document cannot be edited. To change one that is pending, cancel it and start a new draft.

## Creating a document

Choose **New Document** on the **Documents** page. Drop one or more files, or start from a [template](templates.md), and give the document a title.

### Files

- **PDF** files are accepted as they are, after cleaning (see below).
- **Word** (`.docx`, `.doc`), **OpenDocument** (`.odt`) and **RTF** files are converted to PDF by Gotenberg. This needs a converter on the install; without one, only PDFs are accepted.
- A document can have several files. When you send it, they are combined in order into one PDF.
- The largest upload is set by an admin in **Settings → Security** (25 MB by default).

Every PDF is checked by its content, not its name, and rewritten without JavaScript, form actions, embedded files, XFA, and launch or URI actions. If something was removed, OpenSign tells you what. Nobody, including the signers, ever sees the original active content.

### Details

| Setting | |
| --- | --- |
| Title | Shown in the document list, the invitation and on the signing page. |
| Message to Parties | Shown in the invitation and on the signing page. |
| Language for Signers | The language of emails, SMS and the signing page, unless a party has its own. Empty uses the default from **Settings → Organisation**. See [The signing page](signing-page.md#languages). |
| Expires | The date after which the document can no longer be signed. It then becomes **Expired**. |
| Remind Every (Days) | Sends a reminder to parties who have not signed yet, every so many days (up to 60). 0 turns reminders off. |
| Your Reference | An external id, such as a CRM or case number. You can search for it in the document list, and it is included in webhooks and the API. |

### Parties

Add everyone who signs, approves or receives a copy. For each party you choose their role, how their link reaches them, and how they must identify themselves. **Sign in Order** on the same card invites them one step at a time. See [Parties, roles and signing order](parties.md) and [Identification](identification.md).

### Fields

Pick a party in the field toolbar, then place fields on the pages. Each field belongs to one party, who fills it in when signing.

| Field | |
| --- | --- |
| Signature | A drawn or typed signature. |
| Initials | The party's initials. |
| Text | Free text. Can be required and have a label. |
| Date | Filled in with the date of signing. |
| Checkbox | Can be required, or prefilled as checked. |
| Name, Email, Company | Filled in from the party's details. |

Choose **Me, now (prefilled)** in the toolbar to place text that you fill in yourself before sending, such as a contract number. Required fields of this kind must be filled in before the document can be sent.

Fields are stored as fractions of the page, so they stay in place whatever the page size.

## Before you send

The **Review** panel checks the document's setup as you work and lists anything that would stop it from being signed or make the result weaker:

- Errors, which block sending: no file, no signer or approver, a party who gets the link by email or SMS but has no email address or phone number, a party who must confirm with an SMS code but has no phone number, a viewer with fields to fill in, a required sender field that is empty.
- Warnings, worth a look: a signer without a signature field (their signature then appears on the evidence page only), an eID party without a personal number (so anyone with the link and an eID could sign), a party with neither email nor phone (so they will not get the signed copy).

If an admin has set up AI, the same panel can also ask an AI to read the document. See [AI assistant and review](ai.md#document-review).

## Preview

**Preview**, next to **Send for Signing**, shows the signing page as a chosen party will see it, with their fields, language and identification steps. See [The signing page](signing-page.md#previewing-as-a-party).

## AI review by signers

By default, signers can give their own AI assistant a read-only copy of the document to explain it before they sign. Turn **Let Signers Share With Their AI** off under **AI Review by Signers** to prevent it. See [The signing page](signing-page.md#review-with-your-ai).

## Sending

Choose **Send for Signing**. OpenSign combines the files into one PDF, locks it, and notifies the first parties. For parties whose delivery is **Link**, OpenSign shows **Hand Over the Signing Links** with each link to copy. You can get a fresh link later from the party card, and **New Link** revokes the old one first.

## While it is pending

From the document page you can:

- **Remind** everyone whose turn it is and who has not signed yet.
- Get or replace a party's signing link.
- Start **Sign in Person** for a party who signs on your device. See [In-person signing](in-person.md).
- **Cancel** the document, with an optional reason. Every signing link stops working and parties who were notified are told.
- See the **Activity**: every step in the document's audit trail, such as who opened, identified, signed or downloaded it.

## When it is done

Once the last party has signed, OpenSign seals the document in the background, usually within seconds. The document page then offers the sealed PDF and the original as sent. Parties with an email address get the sealed PDF too. See [Sealed PDFs and evidence](sealed-pdfs.md).

Every download is recorded in the audit trail.

## Sharing with colleagues

Documents belong to the person who created them. To let a colleague see one, share it with them, and optionally let them change it. Managers, auditors and admins can see every document without sharing. See [Users and roles](users-and-roles.md).

## Deleting

A document that is not pending can be deleted with its files. Cancel a pending document first. Signed documents can only be deleted by someone who manages all documents, and a document on legal hold cannot be deleted by anyone. Deletion is recorded in the audit trail. Documents can also be deleted automatically by retention rules. See [Retention and legal hold](retention.md).

## Finding documents

The **Documents** page lists the documents you can see, newest first, with tabs per status. Search by title or by your reference.

Sort documents into folders from the folder column beside the list, and archive finished ones so they leave the status tabs. The **Archived** tab lists them. See [Folders and archive](folders.md).
