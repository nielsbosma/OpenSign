# Sealed PDFs and evidence

When the last party has signed, OpenSign turns the document into one sealed PDF. It holds the document with every value and signature stamped in, evidence pages that tell the story of the signing, and a digital seal that shows the file has not changed since. Anyone can check it with an ordinary PDF reader or on the [verify page](verification.md), and it stays verifiable for years, offline.

## What the sealed PDF contains

1. **The document**, exactly as sent, with each party's field values and signatures stamped in place.
2. **Evidence pages** at the end:
   - the document title, its id and your organisation,
   - each party: name, email, role, how they identified (for example "Swedish BankID, personal number 19xxxxxx-1234"), when they signed, and their IP address,
   - the events from the audit trail: sent, opened, identified, signed, and so on, with times,
   - the attachments signers uploaded, with their SHA-256 hashes,
   - the SHA-256 hash of the original PDF, and where to verify the document.
3. **Embedded files**: a JSON evidence attachment with the same facts in machine-readable form, and the [signer attachments](attachments.md).
4. **The seal**: a PAdES signature over the whole file in your organisation's name, with an RFC 3161 time stamp.
5. **Long-term validation data** and an archive time stamp (see below).

## PAdES B-LTA

The seal follows PAdES Baseline B-LTA, the European standard level for documents that must stay verifiable for a long time:

- **B-B**: a CAdES signature over the whole file, by the organisation seal.
- **B-T**: an RFC 3161 time stamp from a trusted time stamp authority, which proves when the seal was made.
- **B-LT**: every certificate the seal and the time stamp depend on, with OCSP responses or CRLs for each, stored in the PDF's document security store. The file can be validated without contacting anyone, even after the certificates expire.
- **B-LTA**: a document time stamp over everything, including the validation data.

OpenSign checks each OCSP response or CRL before storing it: it must be signed by the issuer or its delegated responder, be about the right certificate, and be current. The validation data and the archive time stamp are added as incremental updates, so the bytes the seal covers never change.

If revocation data cannot be fetched, the document is still sealed and the gap is recorded in the audit log. If the time stamp authority cannot be reached after several attempts, the document is sealed without a time stamp rather than left waiting.

## The seal certificate

OpenSign generates the seal key on the server the first time it seals a document, as a self-signed certificate in your organisation's name, and stores it encrypted. **Settings → Signing and Seal** shows the certificate's subject and how long it is valid.

**Replace the seal key** creates a new self-signed certificate in the organisation's current name. New documents are sealed with it. Documents sealed before stay valid and verifiable.

Because the certificate is self-signed, PDF readers show the seal as valid and unchanged, but may say they do not know the signer's identity until you trust the certificate. The verify page checks that your install sealed the exact file.

## Settings

In **Settings → Signing and Seal**:

| Setting | |
| --- | --- |
| Seal Name | Shown as the signer of the seal. Empty uses the organisation name. |
| Signature Reason | The reason recorded in the seal. Default: "Signed with OpenSign". |
| Timestamp Authority (RFC 3161) | The time stamp authority's URL. Default: `http://timestamp.digicert.com`. |

## Getting the sealed PDF

- **Senders**: download it from the document page, which also offers the original as sent.
- **Parties**: everyone with an email address gets it by email when the document is completed. Files up to 10 MB are attached; larger ones come as a download link. The signing page also offers **Download Signed Document** while the link is valid.
- **API**: `GET /api/v1/documents/{id}/file?version=signed`. See [REST API](api.md).
- **Webhooks**: `document.completed` carries the sealed file's SHA-256. See [Webhooks](webhooks.md).

Every download is recorded in the audit trail.
