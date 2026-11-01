# Envio em massa

O envio em massa envia um [modelo](templates.md) a cada linha de uma planilha. Cada linha se torna um documento próprio, com as suas partes e valores de campos, enviado e acompanhado separadamente: um novo contrato de aluguel para cada locatário, uma atualização de política para cada funcionário.

## Antes de começar

- Prepare um modelo com as partes, os campos, a mensagem e a identificação desejados.
- Prepare um arquivo CSV ou Excel (`.xlsx`). Salve antes como `.xlsx` os arquivos `.xls` mais antigos e os arquivos OpenDocument `.ods`. A primeira linha dá nome às colunas. No máximo 1.000 linhas e 5 MB.
- Você precisa de permissão para enviar documentos.

## Enviar

1. Abra o modelo e escolha **Envio em massa** no menu dele.
2. Escolha o arquivo. O OpenSign o lê e mostra cada coluna com o seu primeiro valor e o que ela **Preenche**.
3. Confira o mapeamento. Altere o destino de qualquer coluna ou escolha **Ignorar**.
4. Opcionalmente, defina **Títulos dos documentos**, um padrão com nomes de colunas entre chaves, como `Contrato {Address}`. Vazio usa o título do modelo e o nome da primeira parte.
5. Corrija as linhas que o OpenSign listar em **Corrija estas linhas antes de enviar**.
6. Envie. O OpenSign cria e envia os documentos em segundo plano e abre a página de andamento do envio em massa.

## O que uma coluna pode preencher

| Destino | |
| --- | --- |
| Nome, E-mail, Celular, Empresa ou Número de identificação pessoal de uma parte | Um conjunto para cada parte do modelo. |
| Um campo | Qualquer campo do modelo que tenha rótulo, preenchido como valor do remetente para essa linha. Os números podem ser escritos das duas formas (`1 234,50` ou `1,234.50`), as datas como YYYY-MM-DD ou no formato do campo, os campos de lista pelo texto da opção e várias escolhas separadas por vírgulas. Um valor que quebre as verificações do campo faz essa linha falhar. |
| Título | O título do documento, em vez do padrão. |
| Sua referência | O id externo do documento, como um número de cliente. |
| O idioma de uma parte | O idioma dos e-mails e da página de assinatura desse signatário, como um código, por exemplo `sv` ou `pt-BR`. Veja [A página de assinatura](signing-page.md#languages). |

## Correspondência de colunas

O OpenSign associa as colunas pelo nome, em inglês e em sueco. Por exemplo, "Email", "E-post" e "Mail" correspondem todos ao e-mail de uma parte, e "Mobile", "Mobil" e "Telefon" correspondem ao número de celular. Quando um modelo tem várias partes, uma coluna que começa com o nome da parte, como "Guarantor email", vai para essa parte.

Se um admin configurou a IA, você pode deixar que ela associe as colunas que as regras internas não conseguiram. Ela recebe os nomes das colunas. Também recebe algumas linhas de amostra, mas somente se um admin permitir que o texto dos documentos seja enviado ao provedor de IA. Veja [Assistente e revisão por IA](ai.md).

## Verificações das linhas

Antes de qualquer envio, cada linha é verificada: cada parte precisa de um nome, de um endereço de e-mail válido se recebe o link por e-mail, de um número de celular válido se o recebe por SMS, e assim por diante. Se alguma linha falhar, nada é enviado e o OpenSign lista os problemas pelo número da linha da planilha. Corrija o arquivo e escolha-o de novo.

## Andamento

A página do envio em massa mostra cada linha com o seu destinatário, o seu status e um link para o documento que ela criou. **Envios em massa**, no fim da página **Modelos**, lista as execuções recentes.

**Parar** ignora as linhas que ainda não foram enviadas. Os documentos já enviados continuam aguardando assinatura; cancele-os um a um, se necessário.

**Exportar Dados** baixa o que cada destinatário preencheu, uma linha por documento, em Excel, CSV ou JSON. Veja [Exportar dados preenchidos](data-export.md).

## Privacidade

Os dados das linhas são armazenados criptografados enquanto o envio em massa é executado e descartados de cada linha assim que o documento dela é enviado.

## Pela API

O envio em massa está disponível como `POST /api/v1/bulk-sends/preview` (lê um arquivo e devolve um mapeamento sugerido, sem enviar nada) e `POST /api/v1/bulk-sends` (envia). Veja [API REST](api.md#bulk-send).
