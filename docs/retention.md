# Retention and legal hold

Retention rules delete documents and their files automatically once you no longer need them, which keeps you in line with data protection rules such as the GDPR. A legal hold keeps a document no matter what the rules say.

## Retention rules

Set the rules in **Settings → Retention**. Each is a number of days; 0 keeps documents of that kind forever, which is the default.

| Rule | Counted from |
| --- | --- |
| Delete Signed Documents After (Days) | When the document was completed. |
| Delete Declined, Canceled and Expired Documents After (Days) | When signing stopped. |
| Delete Untouched Drafts After (Days) | The draft's last change. |

A background job checks the rules every few minutes and deletes the documents that are due. **Settings → Retention** shows what the next run would delete. Templates and documents on legal hold are never deleted by a rule.

The document page shows "Kept until …" when a rule will delete a document, so nobody is surprised.

### Previewing the effect

Before tightening a rule, see what it would do: `GET /api/v1/retention/preview` (or the `retention_preview` MCP tool) counts the documents each rule would delete at the next run, and the documents on legal hold. It needs the **settings.manage** permission.

## What deletion does

Deleting a document, by a rule or by hand:

1. removes the document, its parties, fields and evidence from the database,
2. removes its file records in the same transaction, which destroys the wrapped encryption keys, so the encrypted files are unreadable at once,
3. then a background job removes the encrypted objects from the bucket.

Files that another document still uses, such as a template's file, are kept. The audit trail keeps a record of every deletion, with who or which rule deleted what.

## Legal hold

A legal hold keeps a document from every retention rule and from manual deletion, for example during a dispute, an audit or an investigation.

- On the document page, open the **…** menu and choose **Place on Legal Hold**. A reason is required, such as a case or matter reference.
- The document shows **Legal Hold**, with the reason on hover.
- **Release Legal Hold** in the same menu lifts it. The retention rules then apply again, so a document that was already due is deleted at the next run.

Placing and releasing holds needs the **documents.legal_hold** permission, which managers and admins have. Every hold and release is recorded in the audit trail.

From the API: `PUT /api/v1/documents/{id}/legal-hold` with `{"hold": true, "reason": "Case 2026-114"}`. See [REST API](api.md).
