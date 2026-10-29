# Folders and archive

Folders group documents and templates, and the archive keeps finished work out of the way. Both only change where things are listed. They never change who can see a document, how it is kept, or what happens to it.

## Folders

The **Documents** and **Templates** pages have a folder column on the left. **All Documents** (or **All Templates**) lists everything, whatever folder it is in; choosing a folder lists what is directly in it. On narrow screens the folders are a menu above the list.

- Documents and templates have separate folders. A document cannot go in a template folder or the other way round.
- Folders nest up to three levels, such as *Clients / Acme / 2026*.
- Names are unique within a folder, ignoring case.
- A document or template is in at most one folder, or in none.

### Moving things into a folder

- Drag a row onto a folder in the folder column. Drag a selected row to move every selected row. Dropping on **All Documents** takes them out of their folder.
- Or tick rows, open **Actions** and choose **Move to Folder**.
- On a document or template page, **Move to Folder** is in the **More** menu.
- A document made with **New Document** while a folder is open goes in that folder.

Moving works at any status and is recorded in the audit trail as `document.moved`.

### Who can do what

| | Document folders | Template folders |
| --- | --- | --- |
| See the folders | Everyone | Everyone |
| Create a folder | Anyone who can send documents (`documents.create`) | `templates.manage` |
| Rename, move or delete a folder | The person who made it, and anyone with `documents.manage_all` | `templates.manage` |
| Move an item into a folder | Anyone who may change that item | Anyone who may change that template |

Folders are shared with the whole organisation, but **a folder never grants access**. People see only the documents they could see before: their own, those shared with them, or all of them if their role allows. Two people opening the same folder may see different documents in it, and the count next to a folder counts only what you can see. See [Users and roles](users-and-roles.md).

### Deleting a folder

Deleting a folder deletes nothing in it. Its documents, templates and subfolders move up to the folder it was in, or to the top level. If a subfolder's name is already taken there, the deleted folder's name is added to it in brackets.

## Archive

Archive finished documents and templates you no longer use, so the normal lists stay short.

- Documents can be archived once they are **completed**, **declined**, **canceled** or **expired**. Drafts and pending documents cannot.
- Templates can be archived at any time, unless a bulk send from them is still running. An archived template is not offered in **New Document** and cannot be used for a new document or a [bulk send](bulk-send.md) until it is restored.
- The **Archived** tab next to the status tabs lists the archive, and search and folders work there too. Archived documents keep their folder.
- Archive several at once by ticking rows and choosing **Actions → Archive**, or use **Archive** in the **More** menu on a document page. **Restore** brings them back.

Archiving needs the same permission as changing the document or template. It is recorded in the audit trail as `document.archived` and `document.unarchived`.

### Archive and retention

The archive is not a recycle bin and not a separate store. [Retention rules](retention.md) delete archived documents on the same schedule as any other, and a document on legal hold stays on hold. Archived documents are still downloadable and verifiable.

## From the API

| Method | Path | |
| --- | --- | --- |
| GET | `/folders?scope=documents` | List folders: `documents` or `templates`, with `parentId` and `itemCount` |
| POST | `/folders` | Create one. Body: `scope`, `name`, optional `parentId` |
| PUT | `/folders/{id}` | Rename or move it. Body: `name` and `parentId` (`null` for the top level) |
| DELETE | `/folders/{id}` | Delete it; its contents move up a level |
| POST | `/documents/move` | Body: `ids` (up to 200) and `folderId` (`null` for no folder) |
| POST | `/documents/archive` | Body: `ids` (up to 200) and `archived` (`true` to archive, `false` to restore) |

`GET /documents` and `GET /templates` take `folderId` (only what is directly in that folder) and `archived=true` (the archive instead of the normal list). `POST /documents` takes `folderId`. `documents.counts` returns an `archived` count beside the per-status counts. See [REST API](api.md).
