# Documentos

Um documento no OpenSign é composto por um ou mais arquivos a assinar, pelas partes que participam e pelos campos que cada parte preenche. Você o prepara como rascunho, o envia, e o OpenSign cuida do resto até que ele seja assinado, recusado, cancelado ou expire.

## O ciclo de vida de um documento

| Status | O que significa |
| --- | --- |
| **Rascunho** | Você ainda está preparando o documento. Arquivos, partes e campos podem mudar. |
| **Pendente** | Enviado e aguardando assinaturas. Seus arquivos são reunidos em um único PDF que não pode mais ser alterado. |
| **Assinado** | Todos os que precisavam agir assinaram ou aprovaram. O PDF selado está pronto. |
| **Recusado** | Uma parte recusou. A assinatura foi interrompida para todos. |
| **Cancelado** | O remetente (ou um gerente) o interrompeu. |
| **Expirado** | O prazo terminou antes que todos assinassem. |

Um documento enviado não pode ser editado. Para alterar um documento pendente, cancele-o e comece um novo rascunho.

## Criar um documento

Escolha **Novo documento** na página **Documentos**. Solte um ou mais arquivos, ou comece a partir de um [modelo](templates.md), e dê um título ao documento.

### Arquivos

- Arquivos **PDF** são aceitos como estão, após a higienização (veja abaixo).
- Arquivos **Word** (`.docx`, `.doc`), **OpenDocument** (`.odt`) e **RTF** são convertidos para PDF pelo Gotenberg. Isso exige um conversor na instalação; sem ele, apenas PDFs são aceitos.
- Um documento pode ter vários arquivos. Ao enviá-lo, eles são combinados em ordem em um único PDF.
- O maior upload permitido é definido por um admin em **Configurações → Segurança** (25 MB por padrão).

Tags de texto e campos de formulário preenchíveis no arquivo se tornam campos quando você o envia. O OpenSign informa quantos adicionou e quais tags não pôde usar. Veja [Tags de texto, importação de formulários e detecção de campos](text-tags.md).

Todo PDF é verificado pelo conteúdo, não pelo nome, e reescrito sem JavaScript, ações de formulário, arquivos incorporados, XFA e ações de inicialização ou de URI. Se algo foi removido, o OpenSign informa o que foi. Ninguém, nem mesmo os signatários, chega a ver o conteúdo ativo original.

### Detalhes

| Configuração | |
| --- | --- |
| Título | Exibido na lista de documentos, no convite e na página de assinatura. |
| Mensagem para as partes | Exibida no convite e na página de assinatura. |
| Idioma para os signatários | O idioma dos e-mails, SMS e da página de assinatura, a menos que uma parte tenha o seu próprio. Vazio usa o padrão de **Configurações → Organização**. Veja [A página de assinatura](signing-page.md#languages). |
| Expira | A data após a qual o documento não pode mais ser assinado. Ele passa então para **Expirado**. |
| Lembrar a cada (dias) | Envia um lembrete às partes que ainda não assinaram, a cada tantos dias (até 60). 0 desativa os lembretes. |
| Sua referência | Um id externo, como o número de um CRM ou de um caso. Você pode pesquisá-lo na lista de documentos, e ele é incluído nos webhooks e na API. |

### Partes

Adicione todas as pessoas que assinam, aprovam ou recebem uma cópia. Para cada parte, você escolhe a função, como o link chega até ela e como ela precisa se identificar. **Assinar em ordem**, no mesmo cartão, convida as partes uma etapa de cada vez. Veja [Partes, funções e ordem de assinatura](parties.md) e [Identificação](identification.md).

### Campos

Escolha uma parte na aba **Campos** e depois posicione os campos nas páginas. Os campos também podem vir do próprio arquivo: [tags de texto](text-tags.md#text-tags) como `{{Sign;type=signature;role=Tenant}}` e os campos de um [formulário PDF preenchível](text-tags.md#pdf-forms) se tornam campos no upload, e [Detectar campos](text-tags.md#detect-fields) na aba **Campos** sugere campos onde a página tem linhas e caixas a preencher. Cada campo pertence a uma parte, que o preenche ao assinar. Selecione um campo para alterá-lo no painel **Campo**: quem o preenche, seu rótulo, se é obrigatório e as configurações abaixo.

| Campo | |
| --- | --- |
| Assinatura | Uma assinatura desenhada ou digitada. |
| Iniciais | As iniciais da parte. |
| Texto | Texto livre. Pode ter um padrão, um tamanho e um valor padrão. |
| Número | Um número, com valor mínimo e máximo e um [formato](#number-and-date-formats). |
| Lista suspensa | Uma escolha entre as opções de uma lista. |
| Múltipla escolha | Qualquer quantidade de escolhas de uma lista, escritas na página separadas por vírgula. |
| Botão de opção | Um grupo de botões do qual a parte escolhe um. Veja [Grupos de botões de opção](#radio-groups). |
| Caixa de seleção | Pode ser obrigatória ou vir marcada por padrão. |
| Telefone | Um número de telefone com o código do país, armazenado como +46701234567. |
| Seletor de data | Uma data que a parte escolhe em um calendário, com uma data mais antiga e uma mais recente e um [formato](#number-and-date-formats). |
| Imagem | A parte envia uma imagem ou tira uma foto. Ela é desenhada na caixa, ajustada ao tamanho. |
| Arquivo | A parte envia um PDF, JPEG ou PNG diretamente na página. O nome vai na caixa e o arquivo é selado no PDF assinado, como os [anexos](attachments.md). |
| Fórmula | Um número calculado a partir de outros campos, como um total com IVA. Veja [Fórmulas](#formulas). |
| Data | Preenchida com a data da assinatura, no formato que você escolher. |
| Nome, E-mail, Empresa | Preenchidos com os dados da parte. |

Escolha **Eu, agora (pré-preenchido)** na aba **Campos** para colocar um valor que você mesmo preenche antes de enviar, como um número de contrato, um valor ou uma data. Os campos obrigatórios desse tipo precisam ser preenchidos antes que o documento possa ser enviado.

Os campos de texto, número, lista, botão de opção, caixa de seleção, telefone e data de uma parte podem ter um **Valor padrão** com o qual começam. Ative **Somente leitura** e a parte o vê, mas não pode alterá-lo.

#### Verificações e textos de erro

As verificações são executadas na página de assinatura enquanto a parte digita, e de novo no servidor quando ela assina, de modo que um valor que quebre uma verificação nunca chega ao PDF selado.

| Verificação | Aplica-se a | |
| --- | --- | --- |
| Formato permitido | Texto, iniciais, empresa | Uma predefinição (endereço de e-mail, somente dígitos, código postal sueco, número de identificação pessoal ou número de organização) ou uma expressão regular sua. O valor inteiro precisa corresponder. |
| Tamanho mínimo, Tamanho máximo | Texto, iniciais, empresa | Em caracteres. |
| Mínimo, Máximo | Número | Inclusivo. |
| Mais cedo, Mais tarde | Seletor de data | Inclusivo. |
| Obrigatório | Todo campo que a parte preenche | |

O **Texto de erro** substitui o erro padrão pelo seu, para qualquer verificação desse campo, como *Escreva o número do pedido como AB1234*. Sem ele, a parte vê um erro padrão no seu próprio idioma.

Os padrões usam a sintaxe comum de expressões regulares: classes como `[0-9]` e `\d`, repetição como `{3}`, `+` e `?`, alternativas com `|` e grupos. Lookahead e referências retroativas não são suportados, o que mantém todas as verificações rápidas.

#### Formatos de número e de data

O formato determina como um valor é escrito na página; o que é armazenado continua simples (`1234.5`, `2026-12-31`), de modo que as exportações e a API sempre recebem a mesma forma.

Escreva um formato de número como 1234 ficaria nele. `1,234.00` dá uma vírgula entre os milhares e duas casas decimais, `1 234,00 kr` um espaço, uma vírgula decimal e uma moeda depois, e `$1234` nenhuma casa decimal com o símbolo do dólar. Os valores são arredondados para cima na metade, até as casas decimais do formato. As partes podem digitar números das duas formas, como `1 234,5` ou `1,234.50`.

As datas podem ser escritas como YYYY-MM-DD, DD/MM/YYYY, MM/DD/YYYY, DD.MM.YYYY ou D MMM YYYY (31 Dec 2026). Os mesmos formatos valem para a data da assinatura.

#### Grupos de botões de opção

Posicionar um **Botão de opção** adiciona um botão. Selecione-o e escolha **Adicionar um botão ao grupo** para cada opção adicional, depois arraste os botões para onde devem ficar. Cada botão representa uma opção; o grupo compartilha o rótulo e a exigência de resposta. A parte pode escolher um botão do grupo.

#### Campos condicionais

Em **Mostrar Só Quando**, no painel Campo, adicione condições sobre outros campos. O campo só é exibido enquanto todas elas forem verdadeiras, por exemplo *Número da organização* somente quando *Tipo* for *Empresa*, ou uma caixa *Motivo* somente quando a caixa de seleção *Outro* estiver marcada. Os campos com condições são desenhados com borda tracejada no editor.

| Regra | |
| --- | --- |
| É, Não É | Compara com uma opção, um número ou um texto. O texto é comparado sem diferenciar maiúsculas de minúsculas, e os números pelo valor, de modo que 10 e 10.0 são iguais. |
| Está Preenchido, Está Vazio | Para uma caixa de seleção: Está Marcada, Não Está Marcada. |
| Contém | Uma lista de múltipla escolha tem a opção marcada, ou um texto contém as palavras. |
| É Maior Que, É Menor Que | Números e datas (É Depois de, É Antes de). |

Um campo oculto nunca é obrigatório, fica vazio quando a parte assina e não é escrito no PDF assinado. Um campo que depende de um campo oculto o vê como vazio, de modo que a ocultação se propaga em cascata.

As condições se referem aos campos pelo rótulo, portanto o campo observado precisa ter um rótulo, e somente um campo pode tê-lo. Renomear um campo no editor atualiza as condições e fórmulas que o usam. O campo de uma parte pode olhar para os campos da própria parte, para os valores que você pré-preencheu e para os campos das partes que assinam antes, quando **Assinar em ordem** está ativado. O servidor verifica as condições novamente quando a parte assina, de modo que uma parte não consegue pular um campo alterando a página.

#### Fórmulas

Um campo **Fórmula** calcula um número a partir de outros campos enquanto a parte preenche o documento. A parte vê o resultado, mas não pode alterá-lo, e o servidor o calcula de novo a partir dos valores enviados quando ela assina, de modo que o resultado armazenado é confiável. Dê a ele um [formato de número](#number-and-date-formats) para arredondá-lo e escrevê-lo, como `1 234,00 kr`.

Escreva os rótulos de outros campos entre chaves e use `+ - * /` (ou `× ÷`), parênteses e estas funções:

| Função | |
| --- | --- |
| `round(x, 2)` | Arredonda para cima na metade, com 0 a 6 casas decimais; sem um segundo valor, para um número inteiro. |
| `floor(x)`, `ceil(x)`, `abs(x)` | Para baixo, para cima, ou sem o sinal. |
| `min(a, b, …)`, `max(a, b, …)`, `sum(a, b, …)` | |
| `if(condition, then, else)` | As condições comparam com `=`, `!=`, `<`, `<=`, `>` e `>=`, e se combinam com `and(…)`, `or(…)` e `not(…)`. |
| `contains(list, "text")` | Se uma lista de múltipla escolha tem a opção marcada, ou se um texto contém as palavras. |

Exemplos:

- IVA sobre um preço: `{Price} * 0.25`
- Um total com IVA: `round({Hours} * {Rate} * 1.25, 2)`
- Um desconto acima de um valor: `if({Amount} >= 10000, {Amount} * 0.05, 0)`
- Um preço por plano: `if({Plan} = "Pro", 99, 49) * {Seats}`

Um campo vazio conta como 0, uma caixa de seleção marcada como 1. Uma fórmula que divide por zero, ou que usa um texto que não é um número, fica vazia. Uma fórmula que você mesmo preenche só pode usar os seus próprios campos pré-preenchidos e é calculada quando você salva; a fórmula de uma parte pode usar os mesmos campos que uma [condição](#conditional-fields).

Os campos são armazenados como frações da página, de modo que permanecem no lugar qualquer que seja o tamanho da página.

## Antes de enviar

O painel **Revisão** verifica a configuração do documento enquanto você trabalha e lista tudo o que impediria a assinatura ou tornaria o resultado mais fraco:

- Erros, que bloqueiam o envio: nenhum arquivo, nenhum signatário ou aprovador, uma parte que recebe o link por e-mail ou SMS mas não tem endereço de e-mail ou número de telefone, uma parte que precisa confirmar com um código por SMS mas não tem número de telefone, um leitor com campos a preencher, um campo obrigatório do remetente vazio.
- Avisos, que merecem atenção: um signatário sem campo de assinatura (a assinatura dele aparece então somente na página de evidências), uma parte de eID sem número de identificação pessoal (de modo que qualquer pessoa com o link e um eID poderia assinar), uma parte sem e-mail nem telefone (que, portanto, não receberá a cópia assinada).

Se um admin configurou a IA, o mesmo painel também pode pedir a uma IA que leia o documento. Veja [Assistente e revisão por IA](ai.md#document-review).

## Visualização

**Visualizar**, ao lado de **Enviar para assinatura**, mostra a página de assinatura como uma parte escolhida a verá, com seus campos, idioma e etapas de identificação. Veja [A página de assinatura](signing-page.md#previewing-as-a-party).

## Revisão por IA pelos signatários

Por padrão, os signatários podem dar ao seu próprio assistente de IA uma cópia somente leitura do documento para que o explique antes de assinarem. Desative **Permitir que signatários compartilhem com a IA deles**, em **Revisão por IA pelos signatários**, para impedir isso. Veja [A página de assinatura](signing-page.md#review-with-your-ai).

## Envio

Escolha **Enviar para assinatura**. O OpenSign combina os arquivos em um único PDF, o bloqueia e notifica as primeiras partes. Para as partes cuja entrega é **Link**, o OpenSign mostra **Entregar os links de assinatura** com cada link para copiar. Você pode obter um link novo depois, no cartão da parte, e **Novo link** revoga o antigo primeiro.

## Enquanto está pendente

Na página do documento, você pode:

- **Lembrar** todos aqueles cuja vez chegou e que ainda não assinaram.
- Obter ou substituir o link de assinatura de uma parte.
- Iniciar **Assinar presencialmente** para uma parte que assina no seu dispositivo. Veja [Assinatura presencial](in-person.md).
- **Cancelar** o documento, com um motivo opcional. Todos os links de assinatura deixam de funcionar e as partes que foram notificadas são avisadas.
- Ver a **Atividade**: cada etapa da trilha de auditoria do documento, como quem o abriu, se identificou, assinou ou baixou.

## Quando está concluído

Assim que a última parte assina, o OpenSign sela o documento em segundo plano, em geral em poucos segundos. A página do documento passa então a oferecer o PDF selado e o original como foi enviado. As partes com endereço de e-mail também recebem o PDF selado. Veja [PDFs selados e evidências](sealed-pdfs.md).

Cada download é registrado na trilha de auditoria.

Para obter o que as partes preencheram como dados, escolha **Exportar Dados** no menu **⋯**. Veja [Exportar dados preenchidos](data-export.md).

## Compartilhar com colegas

Os documentos pertencem a quem os criou. Para que um colega veja um, compartilhe-o com ele e, se quiser, permita que ele o altere. Gerentes, auditores e admins podem ver todos os documentos sem compartilhamento. Veja [Usuários e funções](users-and-roles.md).

## Exclusão

Um documento que não está pendente pode ser excluído junto com seus arquivos. Cancele primeiro um documento pendente. Documentos assinados só podem ser excluídos por quem gerencia todos os documentos, e um documento sob bloqueio legal não pode ser excluído por ninguém. A exclusão é registrada na trilha de auditoria. Os documentos também podem ser excluídos automaticamente por regras de retenção. Veja [Retenção e bloqueio legal](retention.md).

## Encontrar documentos

A página **Documentos** lista os documentos que você pode ver, do mais novo ao mais antigo, com abas por status. Pesquise pelo título ou pela sua referência.

Organize os documentos em pastas pela coluna de pastas ao lado da lista e arquive os concluídos para que saiam das abas de status. A aba **Arquivado** os lista. Veja [Pastas e arquivo](folders.md).
