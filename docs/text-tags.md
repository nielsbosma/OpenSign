# Text tags, form import and field detection

You do not have to drag every field onto the page. OpenSign can read fields from the file itself in three ways:

- **Text tags** you write into a Word file or PDF, such as `{{Tenant Sign;role=Tenant;type=signature}}`.
- **PDF forms**: the fields of a fillable PDF become OpenSign fields, with the values they were filled in with.
- **Detect Fields** in the editor suggests fields where the page has lines, underscores or boxes to fill in.

Fields made in any of these ways are ordinary fields. You can move, resize, change or remove them before you send.

## Text tags

Write a tag where a field should go, in the text of the document. When you upload the file, OpenSign turns each tag into a field at the tag's place and takes the tag's text out of the page, so signers never see it. The rest of the line stays exactly where it was.

The syntax is the same as DocuSeal's, so templates made for DocuSeal work unchanged:

```
{{Field Name;role=Tenant;type=signature;required=false}}
```

The first part is the field's name, shown as its label. After it come `key=value` attributes separated by semicolons. A tag can leave out the name and start with an attribute, such as `{{type=checkbox}}`.

| Attribute | |
| --- | --- |
| `type` | The kind of field, see below. `text` when left out. |
| `role` | The party who fills it in, by name, such as `Tenant` or `Landlord`. A role the document does not have yet is added as a new party; fill in their details before sending. Without a role, the field goes to the first party who signs. |
| `required` | `false` makes the field optional. Fields are required by default, checkboxes are not. |
| `readonly` | `true` shows the default value to the party without letting them change it. |
| `default` | A value the party starts from. |
| `title` | A label other than the name. |
| `options` | The choices of a `select` or `multiple` field, separated by commas (or by `|`). |
| `option` | The one choice a `radio` button stands for. Radio tags with the same name form one group. |
| `format` | A date format such as `DD/MM/YYYY` or `MM/DD/YYYY`, or a number format: `usd`, `eur`, `gbp`, `sek`, or how 1234 would look, such as `1 234,00 kr`. See [Number and date formats](documents.md#number-and-date-formats). |
| `min`, `max` | The smallest and largest number, or the earliest and latest date as YYYY-MM-DD. |
| `width`, `height` | The field's size in points (1/72 inch). Without them, the field is as wide as the tag and as tall as its text, and signatures, initials and images get a sensible minimum, growing upwards from the line. |

| Type | Field |
| --- | --- |
| `text` | Text |
| `signature` | Signature |
| `initials` | Initials |
| `date` | Date picker |
| `datenow` | Date of signing, filled in automatically |
| `number` | Number |
| `select` | Dropdown |
| `multiple` | Multi-select |
| `radio` | One radio button |
| `checkbox` | Checkbox |
| `phone` | Phone |
| `image`, `file` | Image or file upload |
| `name`, `email`, `company` | Filled in from the party's details (OpenSign only) |

Examples:

```
{{Client Name;role=Client}}
{{Client Sign;type=signature;role=Client}}
{{Start Date;type=date;format=DD/MM/YYYY;role=Client}}
{{Region;type=select;options=North,South,East,West}}
{{Plan;type=radio;option=Basic}} Basic   {{Plan;type=radio;option=Plus}} Plus
{{type=checkbox;role=Client}} I accept the terms
{{Amount;type=number;format=sek;min=0}}
```

A tag that cannot be used is left on the page as text, and OpenSign tells you why when you upload: a type OpenSign does not have (such as DocuSeal's `payment`, `stamp` or `verification`), an attribute without a value, a value the field does not accept, or a tag broken over two lines. Attributes OpenSign has no use for, such as fonts and colours, are ignored.

### Writing tags in Word

Type tags as ordinary text. Keep each tag on one line: shorten the name or make the font smaller if Word wraps it. The tag's font size sets the field's height, so a tag in 8 point text makes a short field. Text tags work in Word, OpenDocument and RTF files on installs with a document converter, and in any PDF whose text can be read. Scanned pages have no text, so they have no tags.

### Turning tags off

Tags are read on every upload. To upload a file whose text happens to contain `{{` and `}}`, turn it off through the API with `textTags=false`. The [Scrive](scrive.md) and [DocuSign](docusign.md) APIs never read tags.

## PDF forms

A fillable PDF form (an AcroForm) becomes fields when you upload it: text boxes become text fields, checkboxes checkboxes, radio buttons a radio group, lists dropdowns or multi-selects, and signature boxes signature fields. Values the form was filled in with become default values, and fields marked required in the form stay required.

A form field whose name starts with a party's name goes to that party: `Tenant.Name` and `Tenant name` go to Tenant. The others go to the first party who signs. The form itself is still removed from the file, as with every upload, so only OpenSign's fields remain. XFA forms are not read.

## Detect Fields

**Detect Fields** on the **Fields** tab looks at the pages for places that want a field:

- lines and runs of underscores to write on, with the label beside or below them,
- empty boxes to tick, with the text next to them as the label.

The label decides the kind: Signature, Underskrift or Firma becomes a signature field, Name or Namnförtydligande a name, Date or Datum the date of signing, Date of birth a date picker, Email, Phone and Company their fields, and anything else text.

Suggestions are drawn with dashed borders and nothing is saved until you accept them. Click one to add it, or **Accept All**. They go to the party chosen on the **Fields** tab; signatures and initials go to a signer. Hover over a suggestion and click the cross to dismiss it. Places that already have a field are not suggested again.

### With AI

When an [AI provider](ai.md) is set up and **Allow document text** is on, Detect Fields also offers **With AI Too**. OpenSign then sends the model each page's text with its positions, and the places it already found, and adds the model's suggestions marked with a sparkle. It sends no images, at most 30 pages and about 40,000 characters, and the call counts against the AI budget. Each AI detection is written to the [audit log](audit-log.md).

## Over the API

`POST /api/v1/documents/{id}/files` and `documents.upload` read text tags and form fields unless `textTags` or `formFields` is `false`. The response's `fields` says how many fields were made from each, which parties were added and which tags could not be used.

`documents.detect_fields` returns suggestions for a draft without saving anything. Pass `ai: true` to ask the AI model too. Place the ones you want with `documents.set_fields`. See [API](api.md).
