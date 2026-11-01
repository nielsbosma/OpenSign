# 导出已填写的数据

签署人在字段中输入的内容最终会进入封印后的 PDF，你也可以把它们作为数据取出：电子表格、JSON、通过 API，或者通过 MCP 交给 AI 代理。

## 在应用中

选择**导出数据**，然后选择 **Excel**、**CSV** 或 **JSON**：

- 在**文档**页面，位于**新建文档**旁边。导出包含列表所显示的文档：你选择的状态标签页、文件夹、搜索和归档标签页，而不仅仅是你看到的这一页。
- 在**文档**页面选中若干行后，在**操作**下，仅导出这些文档。
- 在模板上，位于**⋯**菜单中，导出由它创建的所有文档。
- 在批量发送上，导出它创建的所有文档。
- 在文档上，位于**⋯**菜单中，导出该文档。

一次导出最多包含 5,000 份文档。缩小筛选范围，可以分批导出更多。

## 包含什么

电子表格每份文档一行，最早的在前：

| 列 | |
| --- | --- |
| Document ID, Title, Reference, Status | 文档。Reference 是你的**你的编号**。 |
| Created, Sent, Completed | 以 UTC 表示的时间。 |
| Party 1 Name, Party 1 Email, Party 1 Signed | 按签署顺序排列的每位参与方，然后是 Party 2，依此类推。 |
| 每个字段一列 | 以字段的标签命名。 |

字段值按 OpenSign 存储它们的方式书写，因此在电子表格中可以排序和求和：

| 字段 | 值 |
| --- | --- |
| 数字 | 纯数字，例如 `1234.5`，无论该字段采用什么显示格式。在 Excel 中它是真正的数字。 |
| 公式 | 服务器计算出的数字，与数字字段相同。 |
| 日期选择器和签署日期 | `YYYY-MM-DD`。 |
| 复选框 | `true` 或 `false`。在 Excel 中是真正的 TRUE 或 FALSE。 |
| 下拉列表和单选按钮组 | 所选的选项。单选按钮组是一列，以组的标签命名。 |
| 多项选择 | 所选的选项，以逗号分隔（在 JSON 中为列表）。 |
| 电话 | 带国家代码的号码，例如 `+46701234567`。 |
| 图片和文件上传 | 文件名。文件本身不会导出；请从文档中下载。 |
| 签名 | 省略：参与方的 Signed 列会说明他们何时签署。 |

尚未填写的字段为空，因此等待中的文档也可以导出。参与方签署时被条件隐藏的字段同样为空。

### 列名

列以字段的标签命名，没有标签时以字段的种类命名。当不同参与方有标签相同的字段时，例如每人一个姓名字段，会加上参与方：`Name (Party 1)`、`Name (Party 2)`。同一参与方使用了两次的标签会加上编号：`Note`、`Note (2)`。在模板中给字段起清晰且唯一的标签，列名也就随之清晰。

当你导出来自不同模板的文档时，每个字段标签都会成为一列，没有该字段的文档在该列留空。

## 安全

- 只会导出你能看到的文档。无法访问所有文档的人导出的是他们自己的文档和与他们共享的文档。
- 导出需要 `documents.export` 权限，管理员、经理、发送人和审计员拥有该权限。查看者可以在文档页面上看到已共享文档的值，但不能导出。参见 [用户与角色](users-and-roles.md#permissions)。
- 个人身份号码永远不会被导出，上传的文件只列出名称。
- 以 `=`、`+`、`-` 或 `@` 开头的 CSV 单元格会在前面加上 `'`，这样电子表格会把它显示为文本，而不是作为公式运行。否则，签署人可以在字段中输入 `=HYPERLINK(…)`，使其在你的电脑上运行。负数则不受影响。Excel 文件会把文本写成文本，所以那里不需要任何改动。
- CSV 文件为带字节顺序标记的 UTF-8，因此 Excel 能正确显示 å、ü 和 中。
- 每次导出都会以 `documents.exported` 写入[审计日志](audit-log.md)，包含格式、筛选条件以及所含的文档。

## 通过 API

一份文档的值，带类型：

```sh
curl https://sign.example.com/api/v1/documents/$ID/values -H "Authorization: Bearer $KEY"
```

```json
{
  "id": "019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28",
  "title": "Lease, Storgatan 1",
  "status": "completed",
  "parties": [{ "id": "…", "slot": "Party 1", "role": "signer", "name": "Anna Berg", "email": "anna@example.com", "signedAt": "2026-10-29T10:12:00Z", "…": "…" }],
  "fields": [{ "key": "Monthly rent", "label": "Monthly rent", "kind": "number", "partyId": "…", "fieldIds": ["…"], "value": 12500 }],
  "values": { "Monthly rent": 12500, "Pets": "No", "Rooms": ["Kitchen", "Bath"], "Agree to terms": true }
}
```

`values` 是最快捷的入口：字段键映射到带类型的值。`fields` 包含相同的值，并附带它们的字段 ID、种类和参与方。读取这些值只需要能访问该文档，并且与打开文档一样不会被审计。

一次获取多份文档：

```sh
curl -X POST https://sign.example.com/api/v1/exports -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" -d '{"format": "xlsx", "templateId": "…", "status": "completed"}'
```

请求体接受 `format`（`csv`、`xlsx` 或 `json`）以及 `ids`、`templateId`、`bulkId`、`status`、`search`、`folderId` 和 `archived` 中的任意项（`archived` 为 `true` 时只含已归档的文档，`false` 时排除它们，省略则两者都包含）。响应包含 `name`、`contentType` 和 `count`，以及用于 CSV 和 Excel 的 `contentBase64`，或用于 JSON 的 `documents`（每项如上所示）。

DocuSign 客户端通过 `GET /envelopes/{id}/form_data` 获得同样的值；参见 [DocuSign API](docusign.md)。

这两个调用同时也是 MCP 工具，`documents_values` 和 `documents_export`，因此代理可以回答“这个月有多少租户对养宠物回答了是？”参见 [MCP](mcp.md)。

## 通过 Webhook

`document.completed` 事件带有 `valuesUrl`，即上面的 `/values` 地址，因此集成可以通过一次调用取得签署人输入的全部内容。Webhook 本身从不携带字段值。参见 [Webhook](webhooks.md#events)。
