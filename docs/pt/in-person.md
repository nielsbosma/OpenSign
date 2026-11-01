# Assinatura presencial

A assinatura presencial permite que uma parte assine no seu próprio computador ou tablet, por exemplo em um balcão, em uma reunião ou na casa de um cliente. A parte não precisa de endereço de e-mail nem de telefone.

## Configurar

Na caixa de diálogo da parte, defina **Enviar link por** como **Presencialmente (no seu dispositivo)**. O OpenSign passa então a não enviar nada a essa parte. Você ainda pode pedir que ela se identifique com um código por SMS ou um eID; ela faz isso no seu dispositivo.

## Assinar

1. Envie o documento como de costume.
2. Quando for a vez da parte, abra o documento e escolha **Assinar presencialmente** no cartão dela.
3. O OpenSign abre a página de assinatura com uma tela de entrega: "Entregue o dispositivo para …". Ela explica que tudo o que a pessoa fizer fica registrado nas evidências do documento.
4. A parte confirma com **Eu sou …**, lê o documento, se identifica, se solicitado, e assina.
5. Quando termina, a página pede que ela devolva o dispositivo a você.

## O que é registrado

- Iniciar a assinatura presencial emite um link novo para a parte e desativa todos os anteriores, de modo que um link que você iniciou antes não pode ser reutilizado.
- A trilha de auditoria registra quem iniciou a sessão presencial e para qual parte. As páginas de evidência mostram a identificação, o endereço IP e o navegador da parte, como para qualquer outro signatário.

## Assinar um documento você mesmo

Para um documento que somente você precisa assinar, como uma declaração, uma confirmação de pedido ou um formulário que você preenche para os seus próprios registros, escolha **Assinar eu mesmo** no rascunho em vez de **Enviar**.

1. Escolha como você se identifica: **Meu login no OpenSign** (o padrão) ou um eID como o BankID, quando um admin tiver configurado algum.
2. Escolha **Assinar agora**. Você passa a ser o único signatário: os signatários e aprovadores que já estavam no rascunho são removidos e os campos deles passam a ser seus. As pessoas que recebem uma cópia continuam recebendo.
3. O OpenSign envia o documento e abre a página de assinatura no seu dispositivo, sem tela de entrega. Sua [assinatura salva](account-security.md#saved-signature) é oferecida, se você tiver uma.
4. Quando você assina, o documento é selado e verificável como qualquer outro.

Com **Meu login no OpenSign**, as páginas de evidência dizem "Identified by: Signed-in OpenSign account" com o seu nome e e-mail, e a trilha de auditoria registra o usuário que iniciou o processo. Se você sair antes de assinar, **Assinar presencialmente** no seu cartão de parte abre a página de assinatura de novo.

## Observações

- **Assinar presencialmente** só é oferecido para partes cuja entrega é Presencialmente, enquanto o documento está pendente e é a vez dessa parte.
- Iniciar a assinatura presencial, ou assinar um documento você mesmo, exige um usuário conectado. Nenhum dos dois está disponível com uma chave de API ou pelo MCP.
- A entrega `pad` do Scrive corresponde à assinatura presencial. Veja [Compatibilidade com o Scrive](scrive.md).
