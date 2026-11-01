# A página de assinatura

É isto o que as pessoas para quem você envia documentos veem. Elas não precisam de conta nem de aplicativo, apenas de um navegador. A página funciona em celulares, tablets e computadores, nos modos claro e escuro.

## Abrir o link

O convite chega por e-mail ou SMS, ou diretamente de você. Ele cita a sua organização e a pessoa que enviou o documento, e informa até quando o link funciona.

Se a parte precisa se identificar para ver o documento, ela faz isso primeiro: com um código por SMS, BankID, Freja ou outro eID. Veja [Identificação](identification.md). Caso contrário, o documento abre imediatamente.

Um link incompleto, revogado, expirado ou que pertence a um documento que não está mais pendente mostra uma mensagem clara em vez do documento.

## Ler e preencher

A página mostra:

- de quem é o documento e a mensagem para as partes, se houver,
- o que se pede à parte: assinar, aprovar ou receber uma cópia,
- o documento inteiro, com os campos da própria parte destacados.

A parte preenche os seus campos: texto, números, listas suspensas, listas de múltipla escolha, grupos de botões de opção, caixas de seleção, números de telefone, datas de um seletor de data, imagens e arquivos que ela envia diretamente na página, iniciais e assinaturas. Cada campo é verificado enquanto ela digita, com erros no seu idioma ou o [texto de erro que você escreveu](documents.md#checks-and-error-texts), e ela só pode assinar quando todos os campos passarem. Os campos de nome, e-mail e empresa são preenchidos com os dados dela. Se você pediu [anexos](attachments.md), ela os envia aqui. Se você pediu uma [foto da webcam](identification.md#webcam-photo), ela a tira aqui.

Em um documento que é assinado em ordem, uma parte que abre o link cedo demais vê "Outras pessoas assinam antes de você. Enviaremos um e-mail quando for a sua vez."

## Assinar

Uma assinatura pode ser:

- **Desenhar**: com o dedo, uma caneta ou o mouse, ou
- **Digitar**: a parte digita o nome, que é exibido em um estilo de assinatura, ou
- **Salva**: uma assinatura salva anteriormente, oferecida primeiro quando existe. Os usuários do OpenSign que assinam os próprios documentos recebem a da sua [conta](account-security.md#saved-signature). Os outros signatários podem marcar **Lembrar minha assinatura neste navegador** ao assinar; na próxima vez que uma parte com o mesmo nome assinar nesse navegador, ela será oferecida aqui, com **Esquecer** para removê-la. Ela fica no navegador e nunca é enviada a lugar nenhum até que a parte assine com ela. Nada é lembrado durante a assinatura presencial no dispositivo de outra pessoa.

Antes de assinar, a parte confirma: "Li o documento e concordo em assiná-lo eletronicamente. Isso tem o mesmo valor de uma assinatura em papel." Se a parte precisa se identificar para assinar, faz isso agora. Depois escolhe **Assinar** (ou **Aprovar**, no caso dos aprovadores).

A parte vê uma confirmação. Quando todos assinaram, as partes com endereço de e-mail recebem o PDF selado por e-mail, e a página de assinatura oferece **Baixar documento assinado** enquanto o link ainda for válido.

## Recusar

**Recusar** pede um motivo opcional e interrompe o documento para todos. O remetente é avisado, assim como todas as partes que já tinham um link.

## Idiomas

Os signatários podem ser atendidos em onze idiomas: inglês, chinês (simplificado), hindi, espanhol, francês, árabe, bengali, português, russo, urdu e sueco. O árabe e o urdu são exibidos da direita para a esquerda.

O idioma de cada parte define os e-mails de convite e de lembrete, os textos de SMS e a página de assinatura dela. É o **Idioma** da parte, se você definiu um; caso contrário, o **Idioma para os signatários** do documento; caso contrário, o padrão de **Configurações → Organização**.

A página de assinatura tem um seletor de idioma no cabeçalho. A escolha de um signatário dura apenas para aquela aba do navegador, de modo que não passa para a próxima pessoa em um dispositivo compartilhado.

Alguns textos continuam em inglês ou sueco: as mensagens do próprio BankID, as mensagens de erro do servidor e as páginas de evidência do PDF selado.

## Revisar com a sua IA

Os signatários podem pedir ao seu próprio assistente de IA, como ChatGPT, Claude ou Copilot, que explique o documento antes de assinarem. Em **Revisar com sua IA**, eles escolhem **Criar link de revisão** e depois copiam um prompt pronto ou escolhem **Abrir no ChatGPT**, **Claude** ou **Copilot**, o que abre o assistente com o prompt já preenchido.

O link de revisão não é o link de assinatura:

- É somente leitura. Ele dá ao assistente o texto do documento e o PDF como foi enviado, e não pode assinar, recusar nem alterar nada.
- É exibido uma única vez e exige a mesma identificação que visualizar o documento.
- Expira após os dias definidos em **Configurações → Segurança** (7 por padrão), ou antes, no prazo do documento ou na expiração do link de assinatura, e permite no máximo 100 acessos.
- Deixa de funcionar quando o signatário o revoga, quando você emite um novo link de assinatura ou quando o documento deixa de estar pendente.
- Cada uso fica na trilha de auditoria.

O texto é marcado para o assistente como não confiável, e as páginas com texto oculto, como texto branco ou invisível, são apontadas para que o assistente possa avisar o signatário sobre elas.

Você pode desativar isso por documento em **Revisão por IA pelos signatários** no editor do documento, o que também revoga os links já compartilhados. Um admin pode desativá-lo para todos definindo os dias como 0.

## Visualizar como uma parte

Escolha **Visualizar** no editor do documento para ver a página de assinatura como uma parte escolhida a verá: a sua marca, a mensagem, o idioma dela, os campos, os pedidos de anexos e as etapas de identificação. Nada pode ser assinado nem enviado a partir da visualização, e ela não é registrada como a parte abrindo o documento.

## Marca

A página mostra o nome e o logotipo da sua organização e usa a cor da sua marca, tudo a partir de **Configurações → Organização**. Veja [Logotipo](settings.md#logo).

A página também pode ser exibida dentro do seu próprio aplicativo. Veja [Assinatura incorporada](embedding.md).

## Privacidade e segurança

- A parte secreta do link fica depois do `#`, de modo que não é enviada ao servidor em requisições nem em logs, e a página a remove da barra de endereços depois de carregada.
- As ações do signatário nunca usam uma sessão do OpenSign conectada, mesmo que o signatário por acaso tenha uma no mesmo navegador.
- Cada etapa é registrada em uma trilha de auditoria à prova de adulteração, como a página informa ao signatário.
