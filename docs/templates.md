# Templates

A template is a document you prepare once and send many times: an employment contract, a lease, a consent form. It has the same files, parties, fields, message and settings as a document, but it is never sent itself. Each time you use it, OpenSign makes a new draft from it.

## Who can use templates

- Anyone who can send documents sees every template and can start a document from one.
- Creating and changing templates needs the **templates.manage** permission, which managers and admins have. See [Users and roles](users-and-roles.md).

## Making a template

1. Go to **Templates** and choose **New Template**. OpenSign creates an untitled template and opens it.
2. Give it a title, upload the files, and set the message, language, signing order and reminders as for a document.
3. Add the parties. For a template these are usually placeholders, such as "Tenant" or "Employee", with the role, delivery and identification you want. Whoever uses the template fills in the real names and contact details.
4. Place the fields for each party. [Text tags](text-tags.md) in the file can place them for you: a tag's `role` names the placeholder party, and a role the template does not have yet is added.

A template stays a template; there is no "send" step.

## Using a template

- **One document**: choose **New Document**, pick the template, and give the document a title. OpenSign copies the files, parties (with their attachment requests), fields, message, language, signing order and reminders into a new draft. Fill in the real names and contact details, then send it as usual.
- **Many documents**: use [Bulk send](bulk-send.md) to send the template to every row of a spreadsheet.
- **From the API**: `POST /api/v1/documents` with `templateId`, then set the parties and send. Scrive's `newfromtemplate` endpoint does the same. See [REST API](api.md).

To collect what people filled in on every document made from a template, choose **Export Data** in the template's **⋯** menu. See [Exporting filled-in data](data-export.md).

A document made from a template is independent of it: changing the template later does not change documents already made from it. The template's files are shared, not copied, so they take no extra storage.

Each document shows which template and version it came from under its title, such as "From Lease, version 3".

## Versions

OpenSign keeps a history of each template. A version holds everything a document copies: the title, message, language, signing order, reminders, AI review setting, files, parties with their attachment requests, and every field with its options, validation, format, radio group, read-only flag, conditions and formula.

- **Automatic versions.** When someone uses a template that changed since its newest version, OpenSign saves a new version first. So every document can name the exact version it was made from, without anyone remembering to save.
- **Save Version.** Open **More → Version History** and save the current state with a note, such as "New notice period after the 2027 rent law". Saving again without changes only updates the note.
- **The header** of a template says which version it is at, or "Changed since version 4" when there are unsaved changes. Click it to open the history.

### Version history

**Version History** lists the versions newest first, with who saved them, the note and how many documents were made from each. Pick one to see:

- what changed from the version before it: settings, files, and parties and fields added, removed or changed (for example "Field Address for Tenant: position, required changed"),
- the documents made from it that you can see,
- **Preview**, which shows the version's pages with its fields placed as they were.

When the template has unsaved changes, **Current** at the top shows what changed since the newest version.

### Restoring a version

**Restore** puts the template back the way it was in that version: files, parties, fields and settings. Nothing is lost. Unsaved changes are saved as a version first, and the restore itself becomes a new version marked "Restored from version 2". Documents already made are not touched.

Personal numbers in versions are stored encrypted, like on parties, and shown masked.

## Replacing a file

When the contract text changes but the layout does not, use the replace button next to the file under **Files** instead of removing it and placing the fields again. Upload the new PDF or Word file and OpenSign:

- puts it in the same place among the template's files,
- keeps every field where it was, and moves fields on later files along if the page count changed,
- moves fields that were on pages the new file does not have to its last page and lists them, so you can put them right.

If the new file has [text tags](text-tags.md#text-tags), the tags win: the old file's fields are dropped and the tags placed instead, adding any parties they name.

On a template, the state before the replace is kept as a version, so the old file stays in the history and can be restored. Replacing works on drafts too. From the API, `POST /api/v1/documents/{id}/files/{fileId}/replace` with the file as multipart `file`. See [REST API](api.md#documents).

## Duplicating a template

**More → Duplicate** makes a copy named "Lease (copy)" in the same folder, with everything the template has. The copy starts its own history. From the API, `POST /api/v1/documents` with `isTemplate: true` and the original as `templateId`.

## Folders and archive

Template folders keep a long list of templates in order, such as *HR* and *Sales*. Archive a template you no longer use: it disappears from **New Document** and bulk send until you restore it. See [Folders and archive](folders.md).

## Templates and retention

Retention rules never delete templates, so their versions stay too. Deleting a template deletes its history, and the files only its versions used. See [Retention and legal hold](retention.md).
