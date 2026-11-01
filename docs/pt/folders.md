# Pastas e arquivo

As pastas agrupam documentos e modelos, e o arquivo tira do caminho o trabalho concluído. Ambos só alteram onde as coisas são listadas. Eles nunca mudam quem pode ver um documento, como ele é mantido ou o que acontece com ele.

## Pastas

As páginas **Documentos** e **Modelos** têm uma coluna de pastas à esquerda. **Todos os documentos** (ou **Todos os modelos**) lista tudo, em qualquer pasta; escolher uma pasta lista o que está diretamente nela. Em telas estreitas, as pastas ficam em um menu acima da lista.

- Documentos e modelos têm pastas separadas. Um documento não pode ir para uma pasta de modelos, nem o contrário.
- As pastas se aninham em até três níveis, como *Clientes / Acme / 2026*.
- Os nomes são únicos dentro de uma pasta, sem diferenciar maiúsculas de minúsculas.
- Um documento ou modelo fica em no máximo uma pasta, ou em nenhuma.

### Mover itens para uma pasta

- Arraste uma linha para uma pasta na coluna de pastas. Arraste uma linha selecionada para mover todas as linhas selecionadas. Soltar em **Todos os documentos** os tira da pasta.
- Ou marque linhas, abra **Ações** e escolha **Mover para pasta**.
- Na página de um documento ou modelo, **Mover para pasta** fica no menu **Mais**.
- Um documento criado com **Novo documento** enquanto uma pasta está aberta vai para essa pasta.

Mover funciona em qualquer status e é registrado na trilha de auditoria como `document.moved`.

### Quem pode fazer o quê

| | Pastas de documentos | Pastas de modelos |
| --- | --- | --- |
| Ver as pastas | Todos | Todos |
| Criar uma pasta | Quem pode enviar documentos (`documents.create`) | `templates.manage` |
| Renomear, mover ou excluir uma pasta | Quem a criou e quem tem `documents.manage_all` | `templates.manage` |
| Mover um item para uma pasta | Quem pode alterar esse item | Quem pode alterar esse modelo |

As pastas são compartilhadas com toda a organização, mas **uma pasta nunca concede acesso**. As pessoas veem apenas os documentos que já podiam ver: os seus, os compartilhados com elas ou todos, se a função permitir. Duas pessoas que abrem a mesma pasta podem ver documentos diferentes nela, e a contagem ao lado de uma pasta conta apenas o que você pode ver. Veja [Usuários e funções](users-and-roles.md).

### Excluir uma pasta

Excluir uma pasta não exclui nada do que há nela. Seus documentos, modelos e subpastas sobem para a pasta em que ela estava, ou para o nível superior. Se o nome de uma subpasta já estiver em uso ali, o nome da pasta excluída é acrescentado a ele entre parênteses.

## Arquivo

Arquive documentos concluídos e modelos que você não usa mais, para que as listas normais fiquem curtas.

- Os documentos podem ser arquivados depois de **concluídos**, **recusados**, **cancelados** ou **expirados**. Rascunhos e documentos pendentes não podem.
- Os modelos podem ser arquivados a qualquer momento, a menos que um envio em massa a partir deles ainda esteja em andamento. Um modelo arquivado não é oferecido em **Novo documento** e não pode ser usado para um novo documento nem para um [envio em massa](bulk-send.md) até ser restaurado.
- A aba **Arquivado**, ao lado das abas de status, lista o arquivo, e a pesquisa e as pastas também funcionam ali. Os documentos arquivados mantêm a sua pasta.
- Arquive vários de uma vez marcando linhas e escolhendo **Ações → Arquivar**, ou use **Arquivar** no menu **Mais** na página de um documento. **Restaurar** os traz de volta.

Arquivar exige a mesma permissão que alterar o documento ou o modelo. É registrado na trilha de auditoria como `document.archived` e `document.unarchived`.

### Arquivo e retenção

O arquivo não é uma lixeira nem um armazenamento separado. As [regras de retenção](retention.md) excluem os documentos arquivados no mesmo cronograma que os demais, e um documento sob bloqueio legal continua bloqueado. Os documentos arquivados ainda podem ser baixados e verificados.

## Pela API

| Método | Caminho | |
| --- | --- | --- |
| GET | `/folders?scope=documents` | Lista as pastas: `documents` ou `templates`, com `parentId` e `itemCount` |
| POST | `/folders` | Cria uma. Corpo: `scope`, `name`, `parentId` opcional |
| PUT | `/folders/{id}` | Renomeia ou move. Corpo: `name` e `parentId` (`null` para o nível superior) |
| DELETE | `/folders/{id}` | Exclui; o conteúdo sobe um nível |
| POST | `/documents/move` | Corpo: `ids` (até 200) e `folderId` (`null` para nenhuma pasta) |
| POST | `/documents/archive` | Corpo: `ids` (até 200) e `archived` (`true` para arquivar, `false` para restaurar) |

`GET /documents` e `GET /templates` aceitam `folderId` (apenas o que está diretamente nessa pasta) e `archived=true` (o arquivo em vez da lista normal). `POST /documents` aceita `folderId`. `documents.counts` retorna uma contagem `archived` ao lado das contagens por status. Veja [API REST](api.md).
