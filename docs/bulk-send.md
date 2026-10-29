# Bulk send

Bulk send sends a [template](templates.md) to every row of a spreadsheet. Each row becomes its own document, with its own parties and field values, sent and tracked separately: a new lease for every tenant, a policy update for every employee.

## Before you start

- Prepare a template with the parties, fields, message and identification you want.
- Prepare a CSV, Excel (`.xlsx`, `.xls`) or OpenDocument (`.ods`) file. The first row names the columns. Up to 1,000 rows and 5 MB.
- You need permission to send documents.

## Sending

1. Open the template and choose **Bulk Send** in its menu.
2. Choose the file. OpenSign reads it and shows each column with its first value and what it **Fills In**.
3. Check the mapping. Change any column's target, or choose **Ignore**.
4. Optionally set **Document Titles**, a pattern with column names in braces, such as `Lease {Address}`. Empty uses the template's title and the first party's name.
5. Fix any rows OpenSign lists under **Fix these rows before sending**.
6. Send. OpenSign creates and sends the documents in the background and opens the bulk send's progress page.

## What a column can fill

| Target | |
| --- | --- |
| A party's Name, Email, Mobile, Company or Personal Number | One set for each party in the template. |
| A field | Any template field with a label, filled in as the sender's value for that row. |
| Title | The document title, instead of the pattern. |
| Your Reference | The document's external id, such as a customer number. |
| A party's Language | The language of that signer's emails and signing page, as a code such as `sv` or `pt-BR`. See [The signing page](signing-page.md#languages). |

## Column matching

OpenSign matches columns by name, in English and Swedish. For example "Email", "E-post" and "Mail" all match a party's email, and "Mobile", "Mobil" and "Telefon" match the mobile number. When a template has several parties, a column that starts with the party's name, such as "Guarantor email", goes to that party.

If an admin has set up AI, you can let the AI match the columns the built-in rules could not. It is sent the column names. It also gets a few sample rows, but only if an admin allows document text to be sent to the AI provider. See [AI assistant and review](ai.md).

## Row checks

Before anything is sent, every row is checked: each party needs a name, a valid email address if they get the link by email, a valid mobile number if they get it by SMS, and so on. If any row would fail, nothing is sent and OpenSign lists the problems by spreadsheet row number. Fix the file and choose it again.

## Progress

The bulk send page shows each row with its recipient, its status and a link to the document it created. **Bulk Sends** at the bottom of the **Templates** page lists recent runs.

**Stop** skips the rows that are not sent yet. Documents already sent stay out for signing; cancel those one by one if needed.

## Privacy

Row data is stored encrypted while the bulk send runs, and dropped from each row once its document is sent.

## From the API

Bulk send is available as `POST /api/v1/bulk-sends/preview` (read a file and get a suggested mapping, sending nothing) and `POST /api/v1/bulk-sends` (send). See [REST API](api.md#bulk-send).
