# Links de compartilhamento e códigos QR

Um link de compartilhamento permite que qualquer pessoa inicie sozinha um documento a partir de um modelo: um formulário de associação no seu site, um termo de responsabilidade na recepção, um termo de consentimento impresso como código QR em um cartaz. Quem abre o link informa o nome e o e-mail, confirma o e-mail com um código e depois assina a própria cópia. Nada é criado para um endereço que não foi confirmado.

## Ativar um link de compartilhamento

1. Abra o modelo. Você precisa poder alterá-lo e enviar documentos.
2. Em **Link público**, ative **Compartilhar com link ou código QR**.
3. Em **Preenchido pelo visitante**, escolha a parte do modelo que quem abrir o link passa a ser, como "Cliente". Ela precisa assinar ou aprovar. Todas as outras partes permanecem como estão no modelo, de modo que um cossignatário do seu lado ainda assina depois dela.
4. Copie o link, ou escolha **Código QR** e baixe-o como PNG (para telas e documentos) ou SVG (para impressão).

O link tem a forma `https://sign.example.com/s/Xq3…`. Ele fica desativado até que você o ative, e desativá-lo o interrompe de imediato. Os documentos criados pelo link são enviados em nome do usuário que o ativou e aparecem na lista de documentos dele, com o título do modelo e o nome do visitante.

Os links de compartilhamento exigem que o e-mail esteja configurado, pois todo visitante confirma o seu endereço. Veja [Configurações](settings.md).

## O que o visitante vê

1. Uma página inicial com o seu logotipo e o título e a mensagem do modelo, no idioma do modelo. Ele pode mudar para qualquer um dos [11 idiomas de assinatura](signing-page.md#languages).
2. Ele informa o nome e o e-mail (e o número de telefone, quando a parte recebe códigos ou links por SMS). Um nome contém apenas letras, não números nem endereços, pois vai para o título do documento e para os e-mails sobre ele. As mensagens de texto dos documentos de um link contam para os limites de SMS descritos em [Identificação](identification.md).
3. O OpenSign envia a ele por e-mail um código de seis dígitos que vale por 15 minutos. Ele o digita.
4. O OpenSign cria um documento a partir do modelo, com ele como a parte, e o envia. A página de assinatura abre imediatamente; o mesmo link também é enviado a ele por e-mail, para que possa voltar a ele depois.

Se outras partes assinam antes dele em um documento que assina em ordem, ele é avisado de que receberá um e-mail quando chegar a sua vez.

A identificação definida na parte do modelo continua valendo. Se a parte precisa se identificar com BankID ou com um código por SMS, o visitante faz isso na página de assinatura como qualquer signatário. Veja [Identificação](identification.md).

## Limites

Cada link tem limites que você define em **Link público**:

| Limite | O que faz |
| --- | --- |
| **Por hora** | No máximo esta quantidade de documentos por hora (de 1 a 1000, padrão 10). |
| **No total** | No máximo esta quantidade de documentos ao todo. Deixe vazio para não haver limite. |
| **Último dia** | O link deixa de funcionar após este dia. |

Além desses, o OpenSign protege cada link por conta própria:

- Cada endereço recebe no máximo 3 códigos por hora, e cada link no máximo três vezes o seu limite por hora (no mínimo 20) em códigos, para que um link não possa ser usado para inundar a caixa de entrada de alguém.
- Um código permite 5 tentativas e é usado uma única vez.
- Cada endereço IP tem a taxa de requisições limitada na página inicial e ao pedir ou digitar códigos, como na página de assinatura.
- As solicitações não confirmadas são excluídas após um dia.

## Alterar o endereço

**Novo endereço** dá ao link um endereço novo. O endereço antigo e todos os códigos QR impressos a partir dele deixam de funcionar de imediato. Use-o quando um link foi compartilhado mais longe do que você pretendia.

## Com a API

- `GET /api/v1/templates/{templateId}/share-link`: o link, seus limites e quantos documentos ele criou.
- `PUT /api/v1/templates/{templateId}/share-link` com `enabled`, `partyId`, `perHour`, `maxTotal` e `expiresAt`.
- `POST /api/v1/templates/{templateId}/share-link/rotate`: um novo endereço.

Essas operações também são ferramentas MCP (`share_links_get`, `share_links_set`, `share_links_rotate`). Veja [API REST](api.md) e [MCP](mcp.md).

Os documentos criados por um link de compartilhamento disparam `document.created` com `"source": "share_link"` e o `shareLinkId` do link. Veja [Webhooks](webhooks.md#events).

## O que é registrado

A trilha de auditoria do modelo registra cada alteração no link, cada código enviado (com o endereço de e-mail) e cada documento criado. A trilha de auditoria do novo documento começa com a sua criação a partir do link de compartilhamento.
