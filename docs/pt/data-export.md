# Exportar dados preenchidos

O que os signatários digitam nos campos vai parar no PDF selado, e você também pode extraí-lo como dados: como planilha, como JSON, pela API ou por um agente de IA via MCP.

## Pelo aplicativo

Escolha **Exportar Dados** e depois **Excel**, **CSV** ou **JSON**:

- Na página **Documentos**, ao lado de **Novo documento**. A exportação contém os documentos que a lista mostra: a aba de status, a pasta, a pesquisa e a aba de arquivo que você escolheu, e não apenas a página que você vê.
- Com linhas selecionadas na página **Documentos**, em **Ações**, apenas para esses documentos.
- Em um modelo, no menu **⋯**, para todos os documentos criados a partir dele.
- Em um envio em massa, para todos os documentos que ele criou.
- Em um documento, no menu **⋯**, apenas para esse documento.

Uma exportação contém até 5.000 documentos. Restrinja os filtros para exportar mais em partes.

## O que ela contém

Uma planilha tem uma linha por documento, do mais antigo ao mais novo:

| Colunas | |
| --- | --- |
| Document ID, Title, Reference, Status | O documento. Reference é a sua **Sua referência**. |
| Created, Sent, Completed | Horários em UTC. |
| Party 1 Name, Party 1 Email, Party 1 Signed | Cada parte na ordem de assinatura, depois Party 2 e assim por diante. |
| Uma coluna por campo | Com o nome do rótulo do campo. |

Os valores dos campos são gravados do jeito que o OpenSign os armazena, para que possam ser ordenados e somados em uma planilha:

| Campo | Valor |
| --- | --- |
| Número | Um número simples, como `1234.5`, qualquer que seja o formato de exibição do campo. No Excel, é um número de verdade. |
| Fórmula | O número calculado pelo servidor, como em um campo de número. |
| Seletor de data e data da assinatura | `YYYY-MM-DD`. |
| Caixa de seleção | `true` ou `false`. No Excel, um TRUE ou FALSE de verdade. |
| Lista suspensa e grupo de botões de opção | A opção escolhida. Um grupo de botões de opção é uma coluna, com o nome do rótulo do grupo. |
| Múltipla escolha | As opções escolhidas, separadas por vírgulas (uma lista no JSON). |
| Telefone | O número com o código do país, como `+46701234567`. |
| Imagem e envio de arquivo | O nome do arquivo. O arquivo em si não é exportado; baixe-o a partir do documento. |
| Assinatura | Omitida: a coluna Signed da parte informa quando ela assinou. |

Um campo que ainda não foi preenchido fica vazio, de modo que documentos pendentes também podem ser exportados. O mesmo vale para um campo que uma condição ocultou quando a parte assinou.

### Nomes das colunas

Uma coluna recebe o nome do rótulo do campo, ou o do seu tipo quando não tem rótulo. Quando as partes têm campos com o mesmo rótulo, como um campo Nome cada uma, a parte é acrescentada: `Name (Party 1)`, `Name (Party 2)`. Um rótulo usado duas vezes pela mesma parte recebe um número: `Note`, `Note (2)`. Dê rótulos claros e únicos aos campos no modelo e as colunas seguem.

Quando você exporta documentos de modelos diferentes, cada rótulo de campo se torna uma coluna, e um documento sem esse campo a deixa vazia.

## Segurança

- Somente os documentos que você pode ver são exportados. Quem não tem acesso a todos os documentos exporta os seus e os que foram compartilhados com essa pessoa.
- Exportar exige a permissão `documents.export`, que admins, gerentes, remetentes e auditores têm. Os leitores podem ver os valores de um documento compartilhado na página do documento, mas não podem exportar. Veja [Usuários e funções](users-and-roles.md#permissions).
- Os números de identificação pessoal nunca são exportados, e os arquivos enviados são listados apenas pelo nome.
- Uma célula de CSV que começa com `=`, `+`, `-` ou `@` recebe um `'` no início para que a planilha a mostre como texto em vez de executá-la como fórmula. Caso contrário, um signatário poderia digitar `=HYPERLINK(…)` em um campo e fazê-lo ser executado no seu computador. Números negativos não são alterados. Os arquivos Excel gravam texto como texto, portanto nada precisa mudar ali.
- Os arquivos CSV são UTF-8 com marca de ordem de bytes, para que o Excel mostre å, ü e 中 corretamente.
- Cada exportação é gravada no [log de auditoria](audit-log.md) como `documents.exported`, com o formato, os filtros e os documentos que ela continha.

## Pela API

Os valores de um documento, tipados:

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

`values` é o caminho mais rápido: chaves de campos mapeadas para valores tipados. `fields` traz os mesmos valores com os ids, tipos e partes dos campos. Ler valores exige apenas acesso ao documento e não é auditado, como abrir o documento.

Muitos documentos de uma vez:

```sh
curl -X POST https://sign.example.com/api/v1/exports -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" -d '{"format": "xlsx", "templateId": "…", "status": "completed"}'
```

O corpo aceita `format` (`csv`, `xlsx` ou `json`) e qualquer um destes: `ids`, `templateId`, `bulkId`, `status`, `search`, `folderId` e `archived` (`true` apenas para documentos arquivados, `false` para deixá-los de fora, omitido para ambos). A resposta traz `name`, `contentType` e `count`, além de `contentBase64` para CSV e Excel ou `documents` (cada um como acima) para JSON.

Os clientes do DocuSign obtêm os mesmos valores em `GET /envelopes/{id}/form_data`; veja a [API do DocuSign](docusign.md).

As duas chamadas também são ferramentas MCP, `documents_values` e `documents_export`, para que um agente possa responder "quantos locatários disseram sim para animais de estimação neste mês?" Veja [MCP](mcp.md).

## Com webhooks

O evento `document.completed` traz um `valuesUrl`, o endereço `/values` acima, para que uma integração possa buscar tudo o que os signatários informaram em uma única chamada. Os webhooks em si nunca trazem valores de campos. Veja [Webhooks](webhooks.md#events).
