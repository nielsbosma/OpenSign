# Identificação

Para cada parte, você escolhe com que rigor ela deve comprovar quem é, separadamente para abrir o documento (**Identificar-se para ver**) e para assiná-lo (**Identificar-se para assinar**). O que quer que ela use é registrado na trilha de auditoria e nas páginas de evidência do PDF selado.

## Métodos

| Método | Para ver | Para assinar | Exige |
| --- | --- | --- | --- |
| **Nenhum** | Padrão | | Possuir o link basta para abrir o documento. |
| **Padrão** | | Padrão | Possuir o link. Típico da entrega por e-mail e SMS, já que só o destinatário recebe o link. |
| **Código por SMS** (`sms_pin`) | Sim | Sim | Um provedor de SMS e um número de celular na parte. |
| **BankID sueco** (`se_bankid`) | Sim | Sim | BankID diretamente ou por meio da TIC Identity. |
| **Freja eID** (`freja`) | Sim | Sim | Um contrato com o Freja eID. |
| **Outro eID** (`broker`) | Sim | Sim | Um broker de eID OpenID Connect, como Idura (Criipto) ou Signicat. |

Um método que não está configurado na instalação aparece desativado na caixa de diálogo da parte. O envio é recusado se uma parte usa um método que não está pronto, de modo que um signatário nunca fica preso.

## Foto da webcam

Marque **Pedir identificação por webcam** em um signatário ou aprovador. Antes de poder assinar, ele tira uma foto de si mesmo ao vivo com a câmera do computador ou do celular: **Ligar a câmera**, **Tirar foto** e depois **Usar foto** ou **Tirar outra**. Não há seletor de arquivos.

- A foto é limpa de dados EXIF e de localização, armazenada criptografada e incorporada ao PDF selado com o seu hash SHA-256. A trilha de auditoria registra quando ela foi tirada.
- Na página do documento, o cartão da parte mostra a foto e quando ela foi tirada. Cada visualização é registrada na trilha de auditoria.
- Somente a página de assinatura pode usar a câmera. Todas as outras páginas do OpenSign a bloqueiam.

Uma foto é evidência de quem assinou, não prova de presença ao vivo: um signatário habilidoso poderia enviar uma imagem estática em vez de uma ao vivo. Para uma identificação forte, use um eID.

## Números de identificação pessoal

Para BankID e Freja, informe o **Número de identificação pessoal** na parte. O OpenSign passa então a exigir que o eID nomeie exatamente essa pessoa: uma identificação por qualquer outra pessoa é recusada e registrada na trilha de auditoria. Sem um número de identificação pessoal, qualquer pessoa que possua o link e tenha um eID poderia assinar, e a verificação de configuração avisa sobre isso.

O número de identificação pessoal é armazenado criptografado e exibido mascarado (apenas os quatro últimos dígitos), inclusive no PDF selado que todas as partes recebem. O número completo devolvido pelo eID permanece criptografado no banco de dados. Ao editar uma parte, deixe o campo vazio para manter o número armazenado.

## Identificar-se para ver e para assinar

**Identificar-se para ver** e **Identificar-se para assinar** são duas etapas separadas, mesmo com o mesmo eID. Somente uma identificação feita para assinar informa à pessoa o que ela está assinando (o BankID mostra isso no aplicativo), de modo que uma parte com as duas se identifica duas vezes.

## Códigos por SMS

A parte pede um código na página de assinatura e recebe um código de 6 dígitos por SMS.

- Um código é válido por 10 minutos e permite 5 tentativas.
- No máximo 5 códigos por hora podem ser enviados a uma parte.
- Os códigos são armazenados apenas como um hash com chave.
- O PDF selado mostra o número de telefone somente com os três últimos dígitos.

### Configurar o SMS

Em **Configurações → SMS**, escolha um provedor:

| Provedor | Nome de usuário | Senha |
| --- | --- | --- |
| [46elks](https://46elks.com) | Nome de usuário da API | Senha da API |
| [Twilio](https://www.twilio.com) | Account SID | Auth token |

O **Remetente** é um nome de até 11 letras, ou um número que você possui. O mesmo provedor é usado para a entrega dos links de assinatura por SMS.

### Limitar o custo do SMS

As mensagens de texto custam dinheiro, e os links de compartilhamento permitem que pessoas que você não conhece peçam esse envio. Estas configurações mantêm a conta sob controle:

- **Países permitidos**: os códigos de discagem de país que podem receber SMS, como `46, 47`. Vazio permite todos os países, que é como uma instalação começa, para que nada que já funciona deixe de funcionar. Defina os países para os quais você realmente envia: alguns números no exterior custam muitas vezes o que custa um local.
- **Mensagens por hora** (200 por padrão) e **Mensagens por dia** (1000): para tudo o que a instalação envia. Quando um deles é atingido, as mensagens seguintes são recusadas até que o período passe, e o servidor registra um aviso no log.
- **Mensagens por hora para um link de compartilhamento** (30): para todos os documentos criados a partir de um link de compartilhamento.

0 significa sem limite.

## BankID sueco

O signatário se identifica com o aplicativo BankID: no mesmo dispositivo com **Abrir o BankID neste dispositivo**, ou lendo um código QR com **BankID em outro dispositivo**. Se a parte tem um número de identificação pessoal, somente o BankID dessa pessoa pode responder.

Há duas maneiras de conectar o BankID. As partes escolhem "BankID sueco" em ambas.

### Com o seu próprio contrato com o BankID

1. Em **Configurações → BankID e eID**, ative **BankID sueco**.
2. Para experimentar, ative **Usar o ambiente de teste do BankID** e deixe o certificado vazio. O OpenSign usa o certificado de teste público do BankID, e você pode assinar com um [BankID de teste](https://developers.bankid.com/test-portal).
3. Para produção, desative o ambiente de teste e envie o certificado de parte confiável que o seu banco lhe deu (um arquivo `.p12`) com a respectiva senha.

O OpenSign se comunica com o BankID por TLS mútuo e confia apenas no certificado raiz do próprio BankID, que já vem incorporado. Substitua-o em **BankID Server CA** somente se o BankID mandar.

### Por meio da TIC Identity

A [TIC Identity](https://id.tic.io) oferece o BankID sem que você precise de um contrato próprio com um banco.

1. No portal da TIC, ative o OpenID Connect para o seu tenant e registre `https://<your-install>/api/eid/callback` como URI de redirecionamento.
2. Em **Configurações → BankID e eID**, ative **BankID via TIC Identity** e informe o nome do tenant (como em `https://id.tic.io/<tenant>`), o client id e o client secret.

A TIC é usada sempre que o BankID com o seu próprio certificado está desativado. O OpenSign só confia no número de identificação pessoal sueco da declaração `nin` da TIC e exige que seja válido.

## Freja eID

1. Em **Configurações → BankID e eID**, ative **Freja eID**. Use **Usar o ambiente de teste da Freja** durante os testes.
2. Envie o **Certificado do cliente** da Freja (um arquivo `.pfx`) com a respectiva senha.
3. Adicione o **Certificado de assinatura de resultados da Freja**. O OpenSign recusa qualquer resultado cuja assinatura não confira com ele.

O signatário confirma no aplicativo da Freja.

## Outros eIDs nórdicos por meio de um broker

Um broker OpenID Connect como Idura (Criipto) ou Signicat dá acesso ao BankID norueguês e dinamarquês, ao MitID, aos IDs bancários finlandeses e mais.

1. Crie um aplicativo no broker e registre `https://<your-install>/api/eid/callback` como URI de redirecionamento.
2. Em **Configurações → BankID e eID**, ative **Broker de eID (OpenID Connect)** e informe a **issuer URL**, o **client id** e o **client secret**.
3. Opcionalmente, defina **Escolha de eID (acr_values)** para escolher um eID, como `urn:grn:authn:no:bankid`. Vazio deixa o signatário escolher no broker.

O signatário é enviado ao broker e volta. O OpenSign usa o fluxo de código de autorização com PKCE, state e nonce, e verifica a assinatura, o emissor, o público e a validade do token de ID.

A identificação precisa terminar no navegador em que começou: um cookie de curta duração liga as duas pontas, o que vale também para a TIC Identity, e uma resposta que volte por qualquer outro lugar é recusada e registrada na trilha de auditoria. Mantenha os cookies ativados para o endereço do seu OpenSign.

Quando a página de assinatura está incorporada em outro site, um cookie não pode ser definido de dentro do quadro. O signatário recebe então uma janela no endereço do seu OpenSign que informa para qual documento é a identificação e segue para o broker quando aperta o botão. Isso só é oferecido para partes com entrega por link, em uma instalação que permite a incorporação.

## O que é registrado

Para cada identificação, o OpenSign guarda o método, o nome e o número de identificação pessoal (mascarado) devolvidos pelo eID, a hora, o endereço IP e o navegador. A evidência do eID é armazenada criptografada. Ela aparece na trilha de auditoria e nas páginas de evidência do PDF selado. Veja [PDFs selados e evidências](sealed-pdfs.md).
