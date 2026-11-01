# Login, chaves de acesso e dois fatores

Sua conta é protegida por uma senha e, idealmente, por um segundo fator: um aplicativo autenticador ou uma chave de acesso. Gerencie ambos em **Configurações → Conta**.

## Senhas

- Pelo menos 12 caracteres. Frases-senha longas são bem-vindas.
- As senhas são armazenadas como hash Argon2id. Ninguém, nem mesmo os admins, consegue lê-las.
- Após repetidas falhas de login a partir de um endereço, esse endereço é bloqueado para esta conta por algum tempo, começando em um minuto e crescendo até uma hora. Um dia sem falhas as esquece. Somente falhas vindas de vários endereços ao mesmo tempo podem bloquear a conta para todos, e uma chave de acesso entra mesmo assim. Cada tentativa é registrada.
- **Alterar senha** pede a senha atual e encerra as suas outras sessões.
- **Esqueceu sua senha?**, na página de login, envia por e-mail um link de redefinição que funciona por uma hora. Uma redefinição encerra todas as sessões e revoga as suas chaves de API, e você recebe um e-mail se alguma foi revogada. Se a sua conta tem uma chave de acesso, você ainda precisa dela para entrar depois.

## Dois fatores com um aplicativo autenticador

1. Em **Autenticação em dois fatores**, inicie a configuração e leia o código QR com um aplicativo autenticador, como 1Password, Google Authenticator ou Microsoft Authenticator.
2. Digite o código de 6 dígitos para confirmar.
3. Guarde os **códigos de recuperação**. Cada um permite entrar uma vez se você perder o celular. Eles são exibidos apenas desta vez.

A partir daí, o login pede um código depois da senha. Se você perder o celular e os códigos de recuperação, um admin pode redefinir os seus dois fatores (veja [Usuários e funções](users-and-roles.md)).

## Chaves de acesso

Uma chave de acesso permite entrar com Face ID, Touch ID, Windows Hello, o celular ou uma chave de segurança, sem digitar uma senha.

1. Em **Chaves de acesso**, escolha **Adicionar uma chave de acesso**, dê um nome a ela (como "MacBook") e confirme a sua senha.
2. Siga a instrução do navegador.

Depois, escolha **Entrar com uma chave de acesso** na página de login. Uma chave de acesso sempre verifica você no dispositivo (com impressão digital, rosto ou PIN), portanto vale como dois fatores por si só e nenhum código é pedido. Você pode adicionar várias e remover qualquer uma delas.

Depois que a sua conta tem uma chave de acesso e nenhum aplicativo autenticador, a senha sozinha não permite mais entrar: após a senha, a chave de acesso também é exigida. Se você perder todas as chaves de acesso, um admin pode redefinir os seus dois fatores, o que as remove.

## Confirmar que é você

Criar uma chave de API ou conectar um aplicativo dá acesso duradouro, por isso o OpenSign pede primeiro a sua senha ou uma chave de acesso, a menos que você tenha entrado ou confirmado nos últimos dez minutos.

## Exigir dois fatores para todos

Um admin pode ativar **Exigir autenticação em dois fatores para todos** em **Configurações → Segurança**. Quem não tem um aplicativo autenticador nem uma chave de acesso é então solicitado a configurar um antes de poder fazer qualquer outra coisa. Os clientes MCP conectados com uma conta assim são recusados até que isso seja feito. Recomendamos ativar essa opção.

## Login único

Se a sua organização usa o [login único](single-sign-on.md), escolha o botão dele na página de login. O seu provedor de identidade cuida então da sua senha, do segundo fator e do e-mail, e **Configurações → Conta** não tem chaves de acesso, aplicativo autenticador nem senha para você. As chaves de API e os clientes MCP não estão disponíveis para essas contas.

## Idioma

Em **Configurações → Conta e segurança**, escolha o idioma em que o OpenSign é exibido para você: inglês, chinês (simplificado), hindi, espanhol, francês, árabe, bengali, português, russo, urdu ou sueco. Antes de você entrar, o aplicativo segue o seu navegador, e a página de login tem um seletor. Isso não altera o que os signatários veem; eles recebem o idioma definido no documento ou na parte. Algumas mensagens do servidor continuam em inglês.

## Assinatura salva

Em **Configurações → Conta e segurança → Minha assinatura**, desenhe ou digite a sua assinatura uma vez e, se quiser, adicione suas iniciais. Quando você assina um documento você mesmo, a página de assinatura a oferece em uma aba **Salva**. Ela nunca é aplicada sozinha: você ainda escolhe assinar e pode desenhar ou digitar uma nova em vez dela.

Ela só é oferecida quando o link de assinatura pertence a você: quando você [assina um documento você mesmo](in-person.md#signing-a-document-yourself) ou abre, conectado, um link enviado ao seu próprio endereço de e-mail. Quem recebe o seu dispositivo para a assinatura presencial nunca a vê. O desenho é armazenado criptografado como qualquer outro arquivo, e **Remover** o exclui. Salvá-lo e removê-lo são ações registradas na trilha de auditoria.

Os signatários externos não têm conta, então podem marcar **Lembrar minha assinatura neste navegador**. Isso a mantém apenas no navegador deles, oferecida na próxima vez a uma parte com o mesmo nome; nada sobre eles é armazenado no servidor entre documentos. Veja [A página de assinatura](signing-page.md#signing).

## Sessões

As sessões são cookies do navegador que scripts não conseguem ler, vinculados à sua instalação e armazenados no servidor apenas como um hash com chave. Em **Configurações → Segurança**, um admin define:

| Configuração | Padrão | |
| --- | --- | --- |
| Duração da sessão (horas) | 12 | Por quanto tempo, no máximo, um login dura. |
| Sair após inatividade (minutos) | 60 | Por quanto tempo uma sessão sobrevive sem uso. |

**Sair**, no menu do usuário, encerra a sessão neste navegador.

## Apps conectados por MCP

Em **Apps de IA (MCP)**, você vê o endereço do servidor MCP da sua instalação e os clientes que conectou, como o Claude. Desconecte qualquer um deles a qualquer momento. Veja [MCP e Claude](mcp.md).

## Chaves de API

As chaves de API são gerenciadas em **Configurações → Chaves de API**. Veja [API REST](api.md#api-keys).
