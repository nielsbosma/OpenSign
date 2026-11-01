# Tags de texto, importação de formulários e detecção de campos

Você não precisa arrastar cada campo para a página. O OpenSign pode ler campos do próprio arquivo de três maneiras:

- **Tags de texto** que você escreve em um arquivo Word ou PDF, como `{{Tenant Sign;role=Tenant;type=signature}}`.
- **Formulários PDF**: os campos de um PDF preenchível se tornam campos do OpenSign, com os valores com que foram preenchidos.
- **Detectar campos** no editor sugere campos onde a página tem linhas, sublinhados ou caixas a preencher.

Os campos criados de qualquer uma dessas formas são campos comuns. Você pode movê-los, redimensioná-los, alterá-los ou removê-los antes de enviar.

## Tags de texto

Escreva uma tag onde um campo deve ficar, no texto do documento. Quando você envia o arquivo, o OpenSign transforma cada tag em um campo no lugar dela e retira o texto da tag da página, de modo que os signatários nunca a veem. O restante da linha permanece exatamente onde estava.

A sintaxe é a mesma do DocuSeal, então os modelos feitos para o DocuSeal funcionam sem alterações:

```
{{Field Name;role=Tenant;type=signature;required=false}}
```

A primeira parte é o nome do campo, exibido como seu rótulo. Depois vêm atributos `key=value` separados por ponto e vírgula. Uma tag pode omitir o nome e começar com um atributo, como `{{type=checkbox}}`.

| Atributo | |
| --- | --- |
| `type` | O tipo de campo, veja abaixo. `text` quando omitido. |
| `role` | A parte que o preenche, pelo nome, como `Tenant` ou `Landlord`. Uma função que o documento ainda não tem é adicionada como uma nova parte; preencha os dados dela antes de enviar. Sem uma função, o campo vai para a primeira parte que assina. |
| `required` | `false` torna o campo opcional. Os campos são obrigatórios por padrão; as caixas de seleção, não. |
| `readonly` | `true` mostra o valor padrão à parte sem permitir que ela o altere. |
| `default` | Um valor com o qual a parte começa. |
| `title` | Um rótulo diferente do nome. |
| `options` | As escolhas de um campo `select` ou `multiple`, separadas por vírgulas (ou por `|`). |
| `option` | A única escolha que um botão `radio` representa. As tags radio com o mesmo nome formam um grupo. |
| `format` | Um formato de data como `DD/MM/YYYY` ou `MM/DD/YYYY`, ou um formato de número: `usd`, `eur`, `gbp`, `sek`, ou como 1234 ficaria, como `1 234,00 kr`. Veja [Formatos de número e de data](documents.md#number-and-date-formats). |
| `min`, `max` | O menor e o maior número, ou a data mais antiga e a mais recente no formato YYYY-MM-DD. |
| `width`, `height` | O tamanho do campo em pontos (1/72 de polegada). Sem eles, o campo tem a largura da tag e a altura do seu texto, e assinaturas, iniciais e imagens recebem um mínimo razoável, crescendo para cima a partir da linha. |

| Tipo | Campo |
| --- | --- |
| `text` | Texto |
| `signature` | Assinatura |
| `initials` | Iniciais |
| `date` | Seletor de data |
| `datenow` | Data da assinatura, preenchida automaticamente |
| `number` | Número |
| `select` | Lista suspensa |
| `multiple` | Múltipla escolha |
| `radio` | Um botão de opção |
| `checkbox` | Caixa de seleção |
| `phone` | Telefone |
| `image`, `file` | Envio de imagem ou arquivo |
| `name`, `email`, `company` | Preenchidos com os dados da parte (somente no OpenSign) |

Exemplos:

```
{{Client Name;role=Client}}
{{Client Sign;type=signature;role=Client}}
{{Start Date;type=date;format=DD/MM/YYYY;role=Client}}
{{Region;type=select;options=North,South,East,West}}
{{Plan;type=radio;option=Basic}} Basic   {{Plan;type=radio;option=Plus}} Plus
{{type=checkbox;role=Client}} I accept the terms
{{Amount;type=number;format=sek;min=0}}
```

Uma tag que não pode ser usada é deixada na página como texto, e o OpenSign informa o motivo quando você envia o arquivo: um tipo que o OpenSign não tem (como `payment`, `stamp` ou `verification`, do DocuSeal), um atributo sem valor, um valor que o campo não aceita ou uma tag quebrada em duas linhas. Os atributos que o OpenSign não usa, como fontes e cores, são ignorados.

### Escrever tags no Word

Digite as tags como texto comum. Mantenha cada tag em uma linha: encurte o nome ou diminua a fonte se o Word quebrar a linha. O tamanho da fonte da tag define a altura do campo, então uma tag em texto de 8 pontos produz um campo baixo. As tags de texto funcionam em arquivos Word, OpenDocument e RTF em instalações com um conversor de documentos, e em qualquer PDF cujo texto possa ser lido. Páginas digitalizadas não têm texto, portanto não têm tags.

### Desativar as tags

As tags são lidas em todo upload. Para enviar um arquivo cujo texto por acaso contém `{{` e `}}`, desative a leitura pela API com `textTags=false`. As APIs do [Scrive](scrive.md) e do [DocuSign](docusign.md) nunca leem tags.

## Formulários PDF

Um formulário PDF preenchível (um AcroForm) se torna campos quando você o envia: caixas de texto viram campos de texto, caixas de seleção viram caixas de seleção, botões de opção viram um grupo de botões de opção, listas viram listas suspensas ou de múltipla escolha, e caixas de assinatura viram campos de assinatura. Os valores com que o formulário foi preenchido se tornam valores padrão, e os campos marcados como obrigatórios no formulário continuam obrigatórios.

Um campo de formulário cujo nome começa com o nome de uma parte vai para essa parte: `Tenant.Name` e `Tenant name` vão para Tenant. Os demais vão para a primeira parte que assina. O formulário em si ainda é removido do arquivo, como em todo upload, de modo que só permanecem os campos do OpenSign. Os formulários XFA não são lidos.

## Detectar campos

**Detectar campos**, na aba **Campos**, examina as páginas em busca de lugares que pedem um campo:

- linhas e sequências de sublinhados para escrever, com o rótulo ao lado ou abaixo,
- caixas vazias para marcar, com o texto ao lado como rótulo.

O rótulo define o tipo: Signature, Underskrift ou Firma vira um campo de assinatura, Name ou Namnförtydligande um nome, Date ou Datum a data da assinatura, Date of birth um seletor de data, Email, Phone e Company os campos correspondentes, e qualquer outra coisa vira texto.

As sugestões são desenhadas com bordas tracejadas e nada é salvo até que você as aceite. Clique em uma para adicioná-la, ou em **Aceitar todos**. Elas vão para a parte escolhida na aba **Campos**; assinaturas e iniciais vão para um signatário. Passe o mouse sobre uma sugestão e clique na cruz para dispensá-la. Lugares que já têm um campo não são sugeridos de novo.

### Com IA

Quando um [provedor de IA](ai.md) está configurado e **Permitir texto do documento** está ativado, Detectar campos também oferece **Também com IA**. O OpenSign então envia ao modelo o texto de cada página com suas posições e os lugares que já encontrou, e adiciona as sugestões do modelo marcadas com uma faísca. Ele não envia imagens, no máximo 30 páginas e cerca de 40.000 caracteres, e a chamada conta no orçamento de IA. Cada detecção por IA é gravada no [log de auditoria](audit-log.md).

## Pela API

`POST /api/v1/documents/{id}/files` e `documents.upload` leem tags de texto e campos de formulário, a menos que `textTags` ou `formFields` seja `false`. O campo `fields` da resposta informa quantos campos foram criados a partir de cada fonte, quais partes foram adicionadas e quais tags não puderam ser usadas.

`documents.detect_fields` retorna sugestões para um rascunho sem salvar nada. Passe `ai: true` para consultar também o modelo de IA. Posicione as que você quiser com `documents.set_fields`. Veja [API](api.md).
