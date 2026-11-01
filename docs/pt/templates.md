# Modelos

Um modelo é um documento que você prepara uma vez e envia muitas vezes: um contrato de trabalho, um contrato de aluguel, um termo de consentimento. Ele tem os mesmos arquivos, partes, campos, mensagem e configurações de um documento, mas nunca é enviado em si. Cada vez que você o usa, o OpenSign cria um novo rascunho a partir dele.

## Quem pode usar modelos

- Quem pode enviar documentos vê todos os modelos e pode iniciar um documento a partir de qualquer um deles.
- Criar e alterar modelos exige a permissão **templates.manage**, que gerentes e admins têm. Veja [Usuários e funções](users-and-roles.md).

## Criar um modelo

1. Vá para **Modelos** e escolha **Novo modelo**. O OpenSign cria um modelo sem título e o abre.
2. Dê um título a ele, envie os arquivos e defina a mensagem, o idioma, a ordem de assinatura e os lembretes como em um documento.
3. Adicione as partes. Em um modelo, elas costumam ser marcadores de posição, como "Locatário" ou "Funcionário", com a função, a entrega e a identificação que você quiser. Quem usar o modelo preenche os nomes e os dados de contato reais.
4. Posicione os campos de cada parte. As [tags de texto](text-tags.md) no arquivo podem posicioná-los para você: o `role` de uma tag nomeia a parte de marcação, e uma função que o modelo ainda não tem é adicionada.

Um modelo continua sendo um modelo; não existe uma etapa de "enviar".

## Usar um modelo

- **Um documento**: escolha **Novo documento**, selecione o modelo e dê um título ao documento. O OpenSign copia os arquivos, as partes (com seus pedidos de anexos), os campos, a mensagem, o idioma, a ordem de assinatura e os lembretes para um novo rascunho. Preencha os nomes e os dados de contato reais e depois envie como de costume.
- **Muitos documentos**: use o [Envio em massa](bulk-send.md) para enviar o modelo a cada linha de uma planilha.
- **Deixar que as pessoas o iniciem sozinhas**: ative um [link de compartilhamento](share-links.md). Qualquer pessoa com o link ou seu código QR confirma o e-mail e assina a própria cópia.
- **Pela API**: `POST /api/v1/documents` com `templateId`, depois defina as partes e envie. O endpoint `newfromtemplate` do Scrive faz o mesmo. Veja [API REST](api.md).

Para reunir o que as pessoas preencheram em todos os documentos feitos a partir de um modelo, escolha **Exportar Dados** no menu **⋯** do modelo. Veja [Exportar dados preenchidos](data-export.md).

Um documento feito a partir de um modelo é independente dele: alterar o modelo depois não muda os documentos já criados a partir dele. Os arquivos do modelo são compartilhados, não copiados, portanto não ocupam armazenamento extra.

Cada documento mostra, abaixo do título, de qual modelo e versão veio, como "De Contrato de aluguel, versão 3".

## Versões

O OpenSign mantém um histórico de cada modelo. Uma versão contém tudo o que um documento copia: o título, a mensagem, o idioma, a ordem de assinatura, os lembretes, a configuração de revisão por IA, os arquivos, as partes com seus pedidos de anexos e cada campo com suas opções, validação, formato, grupo de botões de opção, indicador de somente leitura, condições e fórmula.

- **Versões automáticas.** Quando alguém usa um modelo que mudou desde a sua versão mais recente, o OpenSign salva antes uma nova versão. Assim, cada documento pode indicar a versão exata de que foi feito, sem que ninguém precise lembrar de salvar.
- **Salvar versão.** Abra **Mais → Histórico de versões** e salve o estado atual com uma nota, como "Novo prazo de aviso prévio após a lei do aluguel de 2027". Salvar novamente sem alterações apenas atualiza a nota.
- **O cabeçalho** de um modelo indica em que versão ele está, ou "Alterado desde a versão 4" quando há alterações não salvas. Clique nele para abrir o histórico.

### Histórico de versões

O **Histórico de versões** lista as versões da mais nova para a mais antiga, com quem as salvou, a nota e quantos documentos foram feitos a partir de cada uma. Escolha uma para ver:

- o que mudou em relação à versão anterior: configurações, arquivos, e partes e campos adicionados, removidos ou alterados (por exemplo, "Campo Endereço para Locatário: posição, obrigatório alterados"),
- os documentos feitos a partir dela que você pode ver,
- **Visualizar**, que mostra as páginas da versão com seus campos posicionados como eram.

Quando o modelo tem alterações não salvas, **Atual**, no topo, mostra o que mudou desde a versão mais recente.

### Restaurar uma versão

**Restaurar** devolve ao modelo o estado que ele tinha naquela versão: arquivos, partes, campos e configurações. Nada se perde. As alterações não salvas são salvas antes como uma versão, e a própria restauração se torna uma nova versão marcada como "Restaurada da versão 2". Os documentos já criados não são afetados.

Os números de identificação pessoal nas versões são armazenados criptografados, como nas partes, e exibidos mascarados.

## Substituir um arquivo

Quando o texto do contrato muda mas o layout não, use o botão de substituir ao lado do arquivo em **Arquivos** em vez de removê-lo e posicionar os campos de novo. Envie o novo arquivo PDF ou Word e o OpenSign:

- o coloca no mesmo lugar entre os arquivos do modelo,
- mantém cada campo onde estava e desloca os campos dos arquivos seguintes se o número de páginas mudou,
- move os campos que estavam em páginas que o novo arquivo não tem para a sua última página e os lista, para que você possa corrigi-los.

Se o novo arquivo tiver [tags de texto](text-tags.md#text-tags), as tags prevalecem: os campos do arquivo antigo são descartados e as tags são posicionadas no lugar, adicionando as partes que elas nomeiam.

Em um modelo, o estado anterior à substituição é guardado como uma versão, de modo que o arquivo antigo permanece no histórico e pode ser restaurado. A substituição também funciona em rascunhos. Pela API, `POST /api/v1/documents/{id}/files/{fileId}/replace` com o arquivo como `file` multipart. Veja [API REST](api.md#documents).

## Duplicar um modelo

**Mais → Duplicar** cria uma cópia chamada "Contrato de aluguel (cópia)" na mesma pasta, com tudo o que o modelo tem. A cópia inicia o próprio histórico. Pela API, `POST /api/v1/documents` com `isTemplate: true` e o original como `templateId`.

## Pastas e arquivo

As pastas de modelos mantêm em ordem uma longa lista de modelos, como *RH* e *Vendas*. Arquive um modelo que você não usa mais: ele desaparece de **Novo documento** e do envio em massa até que você o restaure. Veja [Pastas e arquivo](folders.md).

## Modelos e retenção

As regras de retenção nunca excluem modelos, portanto as versões deles permanecem também. Excluir um modelo exclui o seu histórico e os arquivos que apenas as suas versões usavam. Veja [Retenção e bloqueio legal](retention.md).
