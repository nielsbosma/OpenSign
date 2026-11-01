# Documentação do OpenSign

O OpenSign é um serviço de assinatura eletrônica de código aberto que você mesmo hospeda. Você envia um documento Word ou PDF, posiciona os campos e o envia. Cada signatário recebe um link, comprova quem é da forma que você escolheu e assina. Quando todos terminam, você recebe um PDF selado que qualquer leitor de PDF consegue verificar, com páginas de evidência que registram quem assinou, quando e como se identificou.

Esta documentação segue o seu idioma, e o menu de idioma no topo o altera. As traduções são automáticas e a versão em inglês é a que prevalece.

Estas páginas cobrem todas as partes do OpenSign, para quem envia documentos, para administradores que mantêm uma instalação e para desenvolvedores que fazem integrações.

## Por onde começar

| Você quer | Leia |
| --- | --- |
| Experimentar o OpenSign ou colocá-lo em produção | [Início rápido](quick-start.md) e depois [Implantação](deployment.md) |
| Terminar de configurar uma instalação nova | [Configuração inicial e e-mail](setup.md) |
| Enviar seu primeiro documento | [Documentos](documents.md) e [Partes, funções e ordem de assinatura](parties.md) |
| Pedir aos signatários que usem BankID, Freja ou códigos por SMS | [Identificação](identification.md) |
| Enviar um modelo para centenas de pessoas | [Envio em massa](bulk-send.md) |
| Conferir se um PDF assinado é genuíno | [Verificar um documento](verification.md) |
| Conectar um CRM ou outro sistema | [API REST](api.md) e [Webhooks](webhooks.md) |
| Salvar PDFs assinados no Google Drive, publicar no Slack, atualizar o HubSpot | [Automações](automations.md) por meio do Composio |
| Migrar do Scrive ou do DocuSign | [Compatibilidade com o Scrive](scrive.md) ou [Compatibilidade com o DocuSign](docusign.md) |
| Trabalhar com o OpenSign a partir do Claude | [MCP e Claude](mcp.md) |
| Entender como o OpenSign protege os documentos | [Modelo de segurança](security.md) |

## O que o OpenSign faz

- **Fluxos de assinatura.** Signatários, aprovadores e leitores, em ordem definida ou todos ao mesmo tempo. Os links vão por e-mail ou SMS, você mesmo os entrega, ou o signatário assina presencialmente no seu dispositivo. Lembretes, datas de expiração e recusa com justificativa já vêm incluídos. Veja [Partes, funções e ordem de assinatura](parties.md).
- **Identificação.** Um link por e-mail, um código por SMS, o BankID sueco (com o seu próprio contrato com o banco ou por meio da TIC Identity), o Freja eID ou outros eIDs nórdicos por meio de um broker OpenID Connect. Com um número de identificação pessoal na parte, somente essa pessoa pode assinar. Veja [Identificação](identification.md).
- **Modelos e envio em massa.** Salve um documento preparado como modelo e envie-o para cada linha de um arquivo CSV ou Excel. Veja [Modelos](templates.md) e [Envio em massa](bulk-send.md).
- **Anexos dos signatários.** Peça arquivos a uma parte, como a cópia de um documento de identidade, que são higienizados e selados no PDF assinado. Veja [Anexos dos signatários](attachments.md).
- **PDFs selados.** Selos PAdES B-LTA com um carimbo de tempo RFC 3161 e tudo o que é necessário para validar o arquivo offline por anos. Veja [PDFs selados e evidências](sealed-pdfs.md).
- **Pastas e arquivo.** Organize documentos e modelos em pastas compartilhadas e arquive o trabalho concluído para manter as listas curtas. Veja [Pastas e arquivo](folders.md).
- **Retenção e bloqueio legal.** Exclua documentos automaticamente após períodos definidos e preserve os que importam com um bloqueio legal. Veja [Retenção e bloqueio legal](retention.md).
- **Usuários e funções.** Admin, gerente, remetente, auditor e leitor, com chaves de acesso ou um aplicativo autenticador para o login em dois fatores. Veja [Usuários e funções](users-and-roles.md).
- **APIs.** Uma API REST com OpenAPI, endpoints compatíveis com o Scrive e o DocuSign, webhooks assinados e um servidor MCP para o Claude e outros clientes de IA. Veja [API REST](api.md).
- **IA, se você quiser.** Um assistente e uma revisão dos documentos antes do envio. Desativada até que um administrador escolha um provedor. Veja [Assistente e revisão por IA](ai.md).
- **Segurança em primeiro lugar.** Criptografia em repouso, uma trilha de auditoria à prova de adulteração, cabeçalhos de segurança web rigorosos e verificações de permissão no servidor em cada chamada. Veja [Modelo de segurança](security.md).

## Como as peças se encaixam

O OpenSign é uma única imagem de contêiner. Ele precisa de um Postgres, de um bucket compatível com S3 e, se você quiser aceitar arquivos Word, de um conversor Gotenberg em uma rede privada. O aplicativo web, as APIs, o servidor MCP e o worker em segundo plano rodam na mesma imagem e usam os mesmos serviços, de modo que uma permissão ou uma regra vale da mesma forma, qualquer que seja a origem da requisição.

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Email, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

O OpenSign tem licença MIT. O código-fonte está no [GitHub](https://github.com/nielsbosma/OpenSign).
