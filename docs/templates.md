# Templates

A template is a document you prepare once and send many times: an employment contract, a lease, a consent form. It has the same files, parties, fields, message and settings as a document, but it is never sent itself. Each time you use it, OpenSign makes a new draft from it.

## Who can use templates

- Anyone who can send documents sees every template and can start a document from one.
- Creating and changing templates needs the **templates.manage** permission, which managers and admins have. See [Users and roles](users-and-roles.md).

## Making a template

1. Go to **Templates** and choose **New Template**. OpenSign creates an untitled template and opens it.
2. Give it a title, upload the files, and set the message, language, signing order and reminders as for a document.
3. Add the parties. For a template these are usually placeholders, such as "Tenant" or "Employee", with the role, delivery and identification you want. Whoever uses the template fills in the real names and contact details.
4. Place the fields for each party.

A template stays a template; there is no "send" step.

## Using a template

- **One document**: choose **New Document**, pick the template, and give the document a title. OpenSign copies the files, parties (with their attachment requests), fields, message, language, signing order and reminders into a new draft. Fill in the real names and contact details, then send it as usual.
- **Many documents**: use [Bulk send](bulk-send.md) to send the template to every row of a spreadsheet.
- **From the API**: `POST /api/v1/documents` with `templateId`, then set the parties and send. Scrive's `newfromtemplate` endpoint does the same. See [REST API](api.md).

A document made from a template is independent of it: changing the template later does not change documents already made from it. The template's files are shared, not copied, so they take no extra storage.

## Templates and retention

Retention rules never delete templates. See [Retention and legal hold](retention.md).
