# 文本标签、表单导入与字段检测

你不必把每个字段都拖到页面上。OpenSign 可以通过三种方式从文件本身读取字段：

- 你写入 Word 文件或 PDF 的**文本标签**，例如 `{{Tenant Sign;role=Tenant;type=signature}}`。
- **PDF 表单**：可填写 PDF 的字段会变成 OpenSign 字段，并带有它们已填入的值。
- 编辑器中的**检测字段**会在页面上有横线、下划线或方框待填写的位置建议字段。

用上述任何方式生成的字段都是普通字段。发送前，你可以移动、调整大小、修改或删除它们。

## 文本标签

在文档文本中字段应该出现的位置写下标签。上传文件时，OpenSign 会在标签所在位置把每个标签变成字段，并从页面中去掉标签文本，因此签署人永远看不到它。该行的其余部分保持原样。

语法与 DocuSeal 相同，所以为 DocuSeal 制作的模板无需修改即可使用：

```
{{Field Name;role=Tenant;type=signature;required=false}}
```

第一部分是字段名称，显示为它的标签。其后是以分号分隔的 `key=value` 属性。标签可以省略名称，直接以属性开头，例如 `{{type=checkbox}}`。

| 属性 | |
| --- | --- |
| `type` | 字段的类型，见下文。省略时为 `text`。 |
| `role` | 填写该字段的参与方，按名称指定，例如 `Tenant` 或 `Landlord`。文档中还没有的角色会被添加为新的参与方；发送前请填写其详细信息。没有角色时，字段归第一位签署的参与方。 |
| `required` | `false` 使字段变为可选。字段默认必填，复选框则默认非必填。 |
| `readonly` | `true` 会向参与方显示默认值，但不允许其修改。 |
| `default` | 参与方起始使用的值。 |
| `title` | 与名称不同的标签。 |
| `options` | `select` 或 `multiple` 字段的选项，以逗号（或 `|`）分隔。 |
| `option` | 一个 `radio` 按钮所代表的那个选项。名称相同的 radio 标签构成一个组。 |
| `format` | 日期格式，例如 `DD/MM/YYYY` 或 `MM/DD/YYYY`，或数字格式：`usd`、`eur`、`gbp`、`sek`，或 1234 的样子，例如 `1 234,00 kr`。参见 [数字与日期格式](documents.md#number-and-date-formats)。 |
| `min`, `max` | 最小和最大的数字，或以 YYYY-MM-DD 表示的最早和最晚日期。 |
| `width`, `height` | 字段的尺寸，单位为点（1/72 英寸）。不指定时，字段与标签一样宽、与其文字一样高；签名、姓名缩写和图片会有合理的最小尺寸，并从横线向上生长。 |

| 类型 | 字段 |
| --- | --- |
| `text` | 文本 |
| `signature` | 签名 |
| `initials` | 姓名缩写 |
| `date` | 日期选择器 |
| `datenow` | 签署日期，自动填入 |
| `number` | 数字 |
| `select` | 下拉列表 |
| `multiple` | 多选 |
| `radio` | 一个单选按钮 |
| `checkbox` | 复选框 |
| `phone` | 电话 |
| `image`, `file` | 图片或文件上传 |
| `name`, `email`, `company` | 根据参与方的详细信息填入（仅限 OpenSign） |

示例：

```
{{Client Name;role=Client}}
{{Client Sign;type=signature;role=Client}}
{{Start Date;type=date;format=DD/MM/YYYY;role=Client}}
{{Region;type=select;options=North,South,East,West}}
{{Plan;type=radio;option=Basic}} Basic   {{Plan;type=radio;option=Plus}} Plus
{{type=checkbox;role=Client}} I accept the terms
{{Amount;type=number;format=sek;min=0}}
```

无法使用的标签会作为文本留在页面上，上传时 OpenSign 会告诉你原因：OpenSign 没有的类型（例如 DocuSeal 的 `payment`、`stamp` 或 `verification`）、没有值的属性、字段不接受的值，或者跨了两行的标签。OpenSign 用不上的属性，例如字体和颜色，会被忽略。

### 在 Word 中编写标签

把标签当作普通文本输入。每个标签保持在一行内：如果 Word 把它换行了，请缩短名称或减小字号。标签的字号决定字段的高度，所以 8 磅文字中的标签会得到一个较矮的字段。在装有文档转换器的实例上，文本标签适用于 Word、OpenDocument 和 RTF 文件，也适用于任何文字可被读取的 PDF。扫描页面没有文字，因此也没有标签。

### 关闭标签

每次上传都会读取标签。如果要上传的文件文本中恰好含有 `{{` 和 `}}`，请通过 API 使用 `textTags=false` 将其关闭。[Scrive](scrive.md) 和 [DocuSign](docusign.md) API 从不读取标签。

## PDF 表单

可填写的 PDF 表单（AcroForm）在上传时会变成字段：文本框变成文本字段，复选框变成复选框，单选按钮变成单选按钮组，列表变成下拉列表或多选，签名框变成签名字段。表单中已填入的值会成为默认值，表单中标记为必填的字段保持必填。

名称以某位参与方的名称开头的表单字段会归该参与方：`Tenant.Name` 和 `Tenant name` 归 Tenant。其余的归第一位签署的参与方。与每次上传一样，表单本身仍会从文件中移除，因此只保留 OpenSign 的字段。XFA 表单不会被读取。

## 检测字段

字段工具栏中的**检测字段**会查看各页面上需要字段的位置：

- 供书写的横线和连续下划线，标签在其旁边或下方，
- 供勾选的空方框，旁边的文字作为标签。

标签决定类型：Signature、Underskrift 或 Firma 变成签名字段，Name 或 Namnförtydligande 变成姓名，Date 或 Datum 变成签署日期，Date of birth 变成日期选择器，Email、Phone 和 Company 变成对应的字段，其他一律为文本。

建议以虚线边框绘制，在你接受之前不会保存任何内容。点击某个建议即可添加，或选择**全部接受**。它们归工具栏中所选的参与方；签名和姓名缩写归签署人。把鼠标悬停在某个建议上并点击叉号即可忽略它。已经有字段的位置不会再次被建议。

### 使用 AI

当 [AI 提供商](ai.md)已设置且**允许发送文档文本用于审阅**已开启时，检测字段还会提供**同时使用 AI**。此时 OpenSign 会把每页的文本及其位置、以及已经找到的位置发送给模型，并添加模型的建议，这些建议带有闪光标记。它不发送图片，最多 30 页和约 40,000 个字符，且该调用计入 AI 预算。每次 AI 检测都会写入[审计日志](audit-log.md)。

## 通过 API

`POST /api/v1/documents/{id}/files` 和 `documents.upload` 会读取文本标签和表单字段，除非 `textTags` 或 `formFields` 为 `false`。响应中的 `fields` 说明每种来源生成了多少个字段、添加了哪些参与方，以及哪些标签无法使用。

`documents.detect_fields` 为草稿返回建议，不保存任何内容。传入 `ai: true` 可同时询问 AI 模型。用 `documents.set_fields` 放置你想要的字段。参见 [API](api.md)。
