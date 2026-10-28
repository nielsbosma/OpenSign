# OpenSign documentation

OpenSign is an open source electronic signing service that you run yourself. You upload a Word or PDF document, place fields, and send it. Each signer gets a link, proves who they are in the way you chose, and signs. When everyone is done you get back a sealed PDF that any PDF reader can verify, with evidence pages that record who signed, when, and how they identified themselves.

These pages cover every part of OpenSign, for people who send documents, for admins who run an install, and for developers who integrate with it.

## Where to start

| You want to | Read |
| --- | --- |
| Try OpenSign or put it into production | [Quick start](quick-start.md), then [Deployment](deployment.md) |
| Finish setting up a fresh install | [First-run setup and email](setup.md) |
| Send your first document | [Documents](documents.md) and [Parties, roles and signing order](parties.md) |
| Ask signers to use BankID, Freja or SMS codes | [Identification](identification.md) |
| Send one template to hundreds of people | [Bulk send](bulk-send.md) |
| Check that a signed PDF is genuine | [Verifying a document](verification.md) |
| Connect a CRM or another system | [REST API](api.md) and [Webhooks](webhooks.md) |
| Move from Scrive or DocuSign | [Scrive compatibility](scrive.md) or [DocuSign compatibility](docusign.md) |
| Work with OpenSign from Claude | [MCP and Claude](mcp.md) |
| Understand how OpenSign protects documents | [Security model](security.md) |

## What OpenSign does

- **Signing flows.** Signers, approvers and viewers, in a set order or all at once. Links go out by email or SMS, you hand them over yourself, or the signer signs in person on your device. Reminders, expiry dates, and declining with a reason are built in. See [Parties, roles and signing order](parties.md).
- **Identification.** An email link, an SMS code, Swedish BankID (with your own bank agreement or through TIC Identity), Freja eID, or other Nordic eIDs through an OpenID Connect broker. With a personal number on the party, only that person can sign. See [Identification](identification.md).
- **Templates and bulk send.** Save a prepared document as a template, then send it to every row of a CSV or Excel file. See [Templates](templates.md) and [Bulk send](bulk-send.md).
- **Signer attachments.** Ask a party for files, such as a copy of an ID, which are cleaned and sealed into the signed PDF. See [Signer attachments](attachments.md).
- **Sealed PDFs.** PAdES B-LTA seals with an RFC 3161 time stamp and everything needed to validate the file offline for years. See [Sealed PDFs and evidence](sealed-pdfs.md).
- **Retention and legal hold.** Delete documents automatically after set periods, and keep the ones that matter with a legal hold. See [Retention and legal hold](retention.md).
- **Users and roles.** Admin, manager, sender, auditor and viewer, with passkeys or an authenticator app for two-factor sign-in. See [Users and roles](users-and-roles.md).
- **APIs.** A REST API with OpenAPI, drop-in Scrive and DocuSign endpoints, signed webhooks, and an MCP server for Claude and other AI clients. See [REST API](api.md).
- **AI, if you want it.** An assistant and a review of documents before you send them. Off until an admin picks a provider. See [AI assistant and review](ai.md).
- **Security first.** Encryption at rest, a tamper-evident audit trail, strict web security headers, and server-side permission checks on every call. See [Security model](security.md).

## How it fits together

OpenSign is one container image. It needs Postgres, an S3-compatible bucket, and, if you want to accept Word files, a Gotenberg converter on a private network. The web app, the APIs, the MCP server and the background worker all run in the same image and use the same services, so a permission or a rule applies in the same way however a request arrives.

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Email, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

OpenSign is MIT licensed. The source is on [GitHub](https://github.com/nielsbosma/OpenSign).
