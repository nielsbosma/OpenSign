# Quick start

This page takes you from nothing to a signed document on your own machine. For a production install, read [Deployment](deployment.md) next.

## 1. Run OpenSign locally

You need Docker and a copy of the repository.

```sh
git clone https://github.com/nielsbosma/OpenSign.git
cd OpenSign
docker compose up -d --wait postgres s3 mailpit converter
docker compose run --rm s3-init
docker compose --profile prod up -d --build
```

The first two commands start Postgres, an S3-compatible store, Gotenberg for Word files and Mailpit, and create the buckets. The last one builds and starts the production image.

- OpenSign: http://localhost:8090
- Mailpit, a local inbox for development: http://localhost:8026

The production image sends real email, so it needs a real email account (step 3). To try OpenSign without one, run it in development mode instead, where every email lands in Mailpit:

```sh
pnpm install
pnpm infra:up
cp .env.example .env
pnpm dev            # web app on http://localhost:5173
```

## 2. Create the first admin

A fresh install prints a **setup code** in its log, so that nobody else can claim it before you do.

```sh
docker compose logs app | grep "setup code"
```

In development mode it is in the terminal where `pnpm dev` runs.

Open OpenSign, enter the setup code, your name, your organisation, your email and a password. See [First-run setup and email](setup.md) for what each step does.

## 3. Connect email

OpenSign sends invitations, codes and signed documents by email, so it asks for an email provider before anyone can send. Enter a Resend API key and a sender address on a domain you verified in Resend. OpenSign sends you a test email and only continues once it went through.

In development mode you can leave the key empty: the test email and everything after it goes to Mailpit.

## 4. Send a document

1. Go to **Documents** and choose **New Document**.
2. Drop a PDF or Word file, give it a title and create it.
3. Under **Parties**, add yourself as a signer with your email address.
4. Pick the party in the field toolbar and place a **Signature** field on the page.
5. Choose **Send for Signing**.

Open the invitation, follow the link, draw or type a signature and sign. A few seconds later the document is **Completed**, and you can download the sealed PDF from the document page.

## 5. Check the result

Open **Verify a Document** in the sidebar (or go to `/verify`) and drop the sealed PDF. OpenSign checks the seal and the time stamps and confirms it sealed exactly this file. A PDF reader that understands signatures, such as Adobe Acrobat, also shows the seal.

## Next steps

- [Deployment](deployment.md): run it for real, on Sliplane or anywhere that runs containers.
- [Identification](identification.md): set up SMS codes, BankID or Freja.
- [Users and roles](users-and-roles.md): invite your colleagues.
- [REST API](api.md): send documents from your own systems.
