# Partes, funções e ordem de assinatura

Uma parte é qualquer pessoa que participa de um documento: alguém que assina, aprova ou apenas recebe a cópia assinada. Você adiciona as partes na página do documento, em **Partes**.

## Dados da parte

| Campo | |
| --- | --- |
| Nome | Obrigatório. Exibido na página de assinatura e nas evidências. |
| Função | Assina, Aprova ou Recebe uma cópia. Veja abaixo. |
| E-mail | Necessário para a entrega por e-mail e para receber a cópia assinada. |
| Celular | Com o código do país, como `+46 70 123 45 67`. Necessário para a entrega por SMS e para os códigos por SMS. |
| Empresa | Opcional. Preenche os campos Empresa. |
| Idioma | Os e-mails, SMS e a página de assinatura dessa parte. **Igual ao documento** por padrão. Veja [A página de assinatura](signing-page.md#languages). |
| Enviar link por | Como a parte recebe o seu link de assinatura. Veja [Entrega](#delivery). |
| Identificar-se para ver | O que a parte precisa fazer antes de poder abrir o documento. Veja [Identificação](identification.md). |
| Identificar-se para assinar | O que a parte precisa fazer antes de poder assinar. Veja [Identificação](identification.md). |
| Pedir identificação por webcam | A parte tira uma foto de si mesma antes de assinar. Veja [Foto da webcam](identification.md#webcam-photo). |
| Número de identificação pessoal | Para a identificação por eID. Quando definido, somente o eID dessa pessoa pode assinar. Armazenado criptografado e exibido mascarado. |
| Ordem de assinatura | Quando o documento é assinado em ordem, os números menores vão primeiro. Veja [Ordem de assinatura](#signing-order). |
| Solicitar anexos | Arquivos que a parte deve ou pode enviar. Veja [Anexos dos signatários](attachments.md). |

## Funções

| Função | O que a parte faz |
| --- | --- |
| **Assina** (signatário) | Lê o documento, preenche seus campos e assina com uma assinatura desenhada ou digitada. |
| **Aprova** (aprovador) | Lê o documento e o aprova. Sem imagem de assinatura, mas a aprovação consta nas evidências como uma assinatura. |
| **Recebe uma cópia** (leitor) | Não age. Recebe o PDF selado quando o documento é concluído. Um leitor não pode ter campos. |

O documento é concluído quando todos os signatários e aprovadores agiram.

## Entrega

| Opção | |
| --- | --- |
| **E-mail** | O padrão. O OpenSign envia por e-mail um convite com o link pessoal, e lembretes, se estiverem ativados. |
| **SMS** | O OpenSign envia o link por mensagem de texto. Exige um provedor de SMS em **Configurações → SMS**. |
| **Link (você o entrega)** | O OpenSign não envia nada. Quando você envia o documento, ele mostra o link para você repassar, por exemplo no seu próprio e-mail ou chat, ou para exibir dentro do seu próprio aplicativo com a [assinatura incorporada](embedding.md). |
| **Presencialmente (no seu dispositivo)** | A parte assina no seu computador ou tablet, iniciado a partir da página do documento. Veja [Assinatura presencial](in-person.md). |
| **Nenhum** | O OpenSign não envia nada e não mostra link. Útil para leitores que só precisam da cópia assinada, que vai para o endereço de e-mail deles. |

As opções que não estão configuradas nesta instalação são exibidas, mas não podem ser escolhidas até que um admin as configure.

### Links de assinatura

Cada parte tem um link pessoal. Ele só vale para essa parte e deixa de funcionar quando:

- o documento é concluído, recusado, cancelado ou expira,
- o tempo de vida do próprio link termina: a data de expiração do documento ou, caso contrário, o número de dias definido em **Configurações → Segurança** (30 por padrão), contado a partir de quando a parte foi notificada pela primeira vez, ou
- um novo link é emitido para a parte, o que revoga o antigo.

A parte secreta do link fica depois do `#`, de modo que nunca chega aos logs do servidor, e a página de assinatura a remove da barra de endereços. Veja [Modelo de segurança](security.md#signing-links).

## Ordem de assinatura

Por padrão, todos são convidados ao mesmo tempo e podem assinar em qualquer ordem. Ative **Assinar em ordem** no cartão **Partes** para convidar as partes uma etapa de cada vez:

- Cada signatário e aprovador mostra o número da sua etapa. Ao ativar a ordem, ela parte da ordem em que as partes estão listadas.
- Use as setas no cartão de uma parte para movê-la para antes ou depois.
- Para que várias partes assinem juntas na mesma etapa, dê a elas o mesmo número de **Ordem de assinatura** em **Editar**. Os números menores vão primeiro.
- Quando todos de uma etapa agiram, o OpenSign convida a etapa seguinte.
- Uma parte que abre o seu link antes da sua vez vê "Outras pessoas assinam antes de você" e recebe um e-mail quando chegar a sua vez.

Os leitores não fazem parte da ordem. Eles recebem o PDF selado no final.

## Recusar

Um signatário ou aprovador pode recusar, com um motivo opcional. Recusar interrompe o documento para todos: ele passa a **Recusado**, todos os links deixam de funcionar, o remetente é avisado e um webhook `document.declined` é disparado com o motivo.

## Alterar as partes

As partes podem ser alteradas enquanto o documento é um rascunho. Remover uma parte também remove os campos dela. Depois que um documento é enviado, suas partes ficam fixas; para alterá-las, cancele o documento e comece um novo.
