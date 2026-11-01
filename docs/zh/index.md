# OpenSign 文档

OpenSign 是一个开源的电子签署服务，由你自己运行。你上传 Word 或 PDF 文档，放置字段，然后发送。每位签署人会收到一个链接，按你选择的方式验证身份，然后签署。所有人完成后，你会得到一份任何 PDF 阅读器都能验证的封印 PDF，其中的证据页记录了谁在何时签署，以及用什么方式验证了身份。

本文档跟随您的语言,可通过顶部的语言菜单更改。译文为机器翻译,请以英文版本为准。

这些页面涵盖 OpenSign 的方方面面，面向发送文档的人、运维安装实例的管理员，以及与其集成的开发者。

## 从哪里开始

| 你想要 | 阅读 |
| --- | --- |
| 试用 OpenSign 或投入生产 | [快速入门](quick-start.md)，然后是 [部署](deployment.md) |
| 完成全新安装的设置 | [首次运行设置与邮件](setup.md) |
| 发送你的第一份文档 | [文档](documents.md) 和 [参与方、角色与签署顺序](parties.md) |
| 要求签署人使用 BankID、Freja 或短信验证码 | [身份验证](identification.md) |
| 把一个模板发送给数百人 | [批量发送](bulk-send.md) |
| 检查已签署的 PDF 是否真实 | [验证文档](verification.md) |
| 连接 CRM 或其他系统 | [REST API](api.md) 和 [Webhook](webhooks.md) |
| 把已签署的 PDF 保存到 Google Drive、发布到 Slack、更新 HubSpot | 通过 Composio 使用 [自动化](automations.md) |
| 从 Scrive 或 DocuSign 迁移 | [Scrive 兼容](scrive.md) 或 [DocuSign 兼容](docusign.md) |
| 在 Claude 中使用 OpenSign | [MCP 与 Claude](mcp.md) |
| 了解 OpenSign 如何保护文档 | [安全模型](security.md) |

## OpenSign 能做什么

- **签署流程。** 签署人、审批人和查看者，可按既定顺序，也可同时进行。链接通过邮件或短信发出，也可由你亲自交给对方，或者让签署人在你的设备上当面签署。内置提醒、过期日期，以及附理由拒绝签署。参见 [参与方、角色与签署顺序](parties.md)。
- **身份验证。** 邮件链接、短信验证码、瑞典 BankID（使用你自己的银行协议，或通过 TIC Identity），Freja eID，以及通过 OpenID Connect 代理接入的其他北欧电子身份。参与方填写了个人身份号码后，只有这个人才能签署。参见 [身份验证](identification.md)。
- **模板与批量发送。** 把准备好的文档保存为模板，然后发送给 CSV 或 Excel 文件中的每一行。参见 [模板](templates.md) 和 [批量发送](bulk-send.md)。
- **签署人附件。** 向参与方索取文件，例如身份证件副本，这些文件会经过清理并封存进已签署的 PDF。参见 [签署人附件](attachments.md)。
- **封印 PDF。** 带有 RFC 3161 时间戳的 PAdES B-LTA 印章，以及离线验证该文件多年所需的全部信息。参见 [封印 PDF 与证据](sealed-pdfs.md)。
- **文件夹与归档。** 把文档和模板整理到共享文件夹中，并归档已完成的工作，让列表保持简短。参见 [文件夹与归档](folders.md)。
- **保留与法律保全。** 在设定的期限后自动删除文档，并用法律保全留住重要的文档。参见 [保留与法律保全](retention.md)。
- **用户与角色。** 管理员、经理、发送人、审计员和查看者，并支持用通行密钥或身份验证器应用进行双重验证登录。参见 [用户与角色](users-and-roles.md)。
- **API。** 带 OpenAPI 的 REST API、可直接替换的 Scrive 和 DocuSign 端点、已签名的 Webhook，以及面向 Claude 和其他 AI 客户端的 MCP 服务器。参见 [REST API](api.md)。
- **AI，按需启用。** 在发送前提供助手和文档审阅。在管理员选定提供商之前保持关闭。参见 [AI 助手与审阅](ai.md)。
- **安全优先。** 静态加密、防篡改的审计日志、严格的 Web 安全响应头，以及每次调用都在服务器端进行的权限检查。参见 [安全模型](security.md)。

## 各部分如何配合

OpenSign 是一个容器镜像。它需要 Postgres、一个兼容 S3 的存储桶，如果要接收 Word 文件，还需要位于专用网络中的 Gotenberg 转换器。Web 应用、各类 API、MCP 服务器和后台 worker 都运行在同一个镜像中并使用相同的服务，因此无论请求以何种方式到达，权限和规则的作用方式都是一样的。

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Email, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

OpenSign 采用 MIT 许可证。源代码在 [GitHub](https://github.com/nielsbosma/OpenSign) 上。
