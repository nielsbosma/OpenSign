# Anexos dos signatários

Você pode pedir a uma parte que envie arquivos enquanto assina: uma cópia de um documento de identidade, um certificado, um extrato bancário. Os arquivos são higienizados, armazenados criptografados e selados no PDF assinado junto com o restante das evidências.

## Solicitar anexos

Na caixa de diálogo da parte, em **Solicitar anexos**, adicione uma linha para cada arquivo desejado:

| Campo | |
| --- | --- |
| Nome | O que você está pedindo, como "Cópia do documento de identidade". Exibido à parte. |
| Descrição | Opcional. Mais detalhes para a parte. |
| Obrigatório | Um anexo obrigatório precisa ser enviado antes que a parte possa assinar. Os opcionais podem ser ignorados. |

Você pode pedir até 10 anexos a cada parte. Os pedidos de anexos são copiados quando um documento é criado a partir de um [modelo](templates.md) e podem ser definidos pela API com a lista `attachments` da parte.

## O que o signatário vê

A página de assinatura lista os arquivos solicitados em **Anexos**, cada um com **Enviar**, **Substituir** e **Remover**. A parte pode alterar os envios até assinar.

- Tipos aceitos: PDF, JPEG e PNG, de até 10 MB cada.
- Os arquivos são verificados pelo conteúdo, não pelo nome.

## Como os arquivos são higienizados

- Os **PDFs** são reescritos como qualquer outro upload: sem JavaScript, ações de formulário, arquivos incorporados nem XFA, e com no máximo 50 páginas.
- As **imagens** são decodificadas e codificadas novamente como PNG. Isso elimina todos os metadados, como posições de GPS em fotos de celular, e qualquer coisa escondida no arquivo. Imagens muito grandes (acima de 40 megapixels) são recusadas.
- Qualquer outra coisa é recusada.

## No PDF selado

Quando o documento é selado, cada anexo é:

- listado nas páginas de evidência com a parte, o nome que você pediu, o nome do arquivo e o seu hash SHA-256, e
- incorporado ao PDF selado como um arquivo anexado, de modo que viaja com o documento e é coberto pelo selo.

Abra o painel de anexos do PDF selado no seu leitor de PDF para vê-los. Veja [PDFs selados e evidências](sealed-pdfs.md).
