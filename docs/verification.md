# Verifying a document

Anyone holding a signed PDF can check that it is genuine and unchanged. No account is needed.

## On the verify page

Open `https://<your-install>/verify` (also linked as **Verify a Document** in the app's sidebar, and printed on every sealed document's evidence pages) and drop the PDF.

The file is checked on the server and not stored. The page answers two questions:

1. **Did this install seal exactly this file?** "Sealed by this OpenSign" with the document's title and completion time means the file's SHA-256 matches a document this server sealed. "Not a document this server sealed" means it was changed after sealing, or it comes from somewhere else.
2. **Are the signatures intact?** Each signature and time stamp in the file is listed with who made it, when, and whether it is intact.

## What the check covers

For every signature and document time stamp in the file, OpenSign checks:

- that the signature's cryptography is valid and covers the bytes it claims to,
- whether it covers the whole file, or only an earlier revision,
- whether anything added after it is only validation data: a document security store with certificates and revocation answers, and invisible document time stamps,
- the time stamp authority's own signature (RSA of 2048 bits or more, ECDSA P-256 and P-384, SHA-2).

Any other change after a signature, such as a new annotation, a form edit or a changed catalog entry, marks that signature as not intact. That closes the common trick of changing a signed PDF with an incremental update that some readers do not flag.

## In a PDF reader

Sealed documents are standard PAdES files, so Adobe Acrobat, Foxit and other readers that understand signatures show the seal, the time stamp and whether the document changed. Since the seal certificate is self-signed by default, a reader may say it does not know the signer's identity; the verify page above is the authoritative check for your install. See [Sealed PDFs and evidence](sealed-pdfs.md#the-seal-certificate).

## From the API

`POST /api/v1/verify` with the raw PDF as the body (up to 40 MB) returns the same result as JSON. It needs no API key and is rate-limited per address. See [REST API](api.md).

```sh
curl -X POST https://sign.example.com/api/v1/verify \
  -H "Content-Type: application/pdf" --data-binary @signed.pdf
```
