# Signer attachments

You can ask a party to upload files while they sign: a copy of an ID, a certificate, a bank statement. The files are cleaned, stored encrypted, and sealed into the signed PDF with the rest of the evidence.

## Asking for attachments

In the party dialog, under **Ask for Attachments**, add one line per file you want:

| Field | |
| --- | --- |
| Name | What you ask for, such as "Copy of ID". Shown to the party. |
| Description | Optional. More detail for the party. |
| Required | A required attachment must be uploaded before the party can sign. Optional ones can be skipped. |

You can ask each party for up to 10 attachments. Attachment requests are copied when a document is made from a [template](templates.md), and can be set through the API with the party's `attachments` list.

## What the signer sees

The signing page lists the requested files under **Attachments**, each with **Upload**, **Replace** and **Remove**. The party can change their uploads until they sign.

- Accepted types: PDF, JPEG and PNG, up to 10 MB each.
- Files are checked by their content, not their name.

## How files are cleaned

- **PDFs** are rewritten like any other upload: no JavaScript, form actions, embedded files or XFA, and at most 50 pages.
- **Images** are decoded and encoded again as PNG. This drops all metadata, such as GPS positions in phone photos, and anything hidden in the file. Very large images (over 40 megapixels) are refused.
- Anything else is refused.

## In the sealed PDF

When the document is sealed, each attachment is:

- listed on the evidence pages with the party, the name you asked for, the file name and its SHA-256 hash, and
- embedded in the sealed PDF as a file attachment, so it travels with the document and is covered by the seal.

Open the sealed PDF's attachments panel in your PDF reader to see them. See [Sealed PDFs and evidence](sealed-pdfs.md).
