# Configuração inicial e e-mail

Uma instalação nova guia você por dois passos antes que alguém possa usá-la: criar o primeiro admin e depois conectar o e-mail. Até que os dois estejam concluídos, o OpenSign leva todos os admins de volta à página de configuração.

## Passo 1: o primeiro admin

Quando o OpenSign inicia e não tem nenhum usuário, ele imprime no log um **código de configuração** de uso único:

```
No admin yet. Open https://sign.example.com and enter the setup code …
```

Abra a instalação em um navegador e preencha:

| Campo | |
| --- | --- |
| Código de configuração | O código do log. Ele prova que você tem acesso ao servidor, de modo que ninguém que encontre uma instalação nova na internet possa reivindicá-la. |
| Organização | O nome da sua organização. Os signatários o veem nos e-mails e na página de assinatura. Você pode alterá-lo depois em **Configurações → Organização**. |
| Seu nome, E-mail, Senha | Sua conta de admin. |

As instalações feitas pelo botão **Deploy to Sliplane** dispensam o código: o instalador gera um `SETUP_CODE`, e **Abrir o OpenSign** na página final dele leva a `/setup#code=…`, que o preenche. O código fica no fragmento da URL, que o navegador nunca envia a um servidor, e a página o remove da barra de endereços. Se você abrir a instalação de outra forma, o código aparece na página final do instalador e no ambiente do serviço no Sliplane. Com `SETUP_CODE` definido, o log deixa de imprimir um código.

Depois que o primeiro admin existe, o código de configuração deixa de funcionar para sempre. Todas as outras pessoas são convidadas em **Configurações → Usuários** (veja [Usuários e funções](users-and-roles.md)).

## Passo 2: e-mail

O OpenSign envia por e-mail convites de assinatura, lembretes, avisos de que um documento foi interrompido, os documentos selados, convites de usuários e redefinições de senha. A configuração pede um provedor de e-mail funcional, o Resend ou o seu próprio servidor SMTP, e envia uma mensagem de teste. A configuração só termina quando o provedor a aceita, e os documentos não podem ser enviados por e-mail antes disso.

### Resend

O [Resend](https://resend.com) é o provedor padrão.

1. Crie uma conta no Resend e verifique o domínio de onde deseja enviar em **Domains**.
2. Crie uma chave de API em **API Keys**, com permissão para enviar.
3. No OpenSign, informe a chave de API, um **Endereço do remetente** no domínio verificado (como `sign@example.com`) e um **Nome do remetente**.

### SMTP

Escolha a aba **Servidor SMTP** durante a configuração, ou **SMTP** como **Provedor** em **Configurações → E-mail**, para enviar pelo seu próprio servidor de e-mail ou pelo relay SMTP de outro provedor.

| Campo | |
| --- | --- |
| Servidor SMTP | O nome do host, como `smtp.example.com`. Sem `smtp://` e sem porta. |
| Porta | Em geral 587 para STARTTLS ou 465 para TLS. |
| Segurança | **STARTTLS** ou **TLS**. |
| Nome de usuário, Senha | Um nome de usuário vazio envia sem autenticação. A senha é armazenada criptografada. |
| Endereço do remetente, Nome do remetente | O remetente. |

O TLS é sempre exigido e o certificado precisa ser válido para o nome do servidor, de modo que as credenciais nunca trafegam sem criptografia. Não existe opção de texto simples. Servidores em redes privadas são recusados, exceto em desenvolvimento.

### Alterar o e-mail depois

As configurações de e-mail ficam em **Configurações → E-mail**. Alterar o provedor, a chave, o servidor SMTP ou o remetente limpa o estado "verificado", e o OpenSign pede um novo e-mail de teste antes que os documentos possam ser enviados novamente. **Responder para** é opcional: vazio, as respostas vão para a pessoa que enviou cada documento.

## O que configurar em seguida

Toda a outra configuração é feita no navegador, em **Configurações**. Nenhuma delas é necessária para começar a enviar documentos com links por e-mail.

- **Organização**: nome, número de registro, país, fuso horário, idioma padrão dos signatários, cor da marca e logotipo. Veja [Configurações de admin](settings.md).
- **SMS**: para convites e códigos de uso único por SMS. Veja [Identificação](identification.md#sms-codes).
- **BankID e eID**: BankID sueco, Freja eID e outros eIDs nórdicos. Veja [Identificação](identification.md).
- **Assinatura e selo**: o nome do selo, o motivo e a autoridade de carimbo de tempo. Veja [PDFs selados e evidências](sealed-pdfs.md).
- **Segurança**: exija login em dois fatores para todos. Veja [Login, chaves de acesso e dois fatores](account-security.md).
- **IA**: opcional. Veja [Assistente e revisão por IA](ai.md).

## Fixar configurações com variáveis de ambiente

Qualquer configuração de admin pode ser definida com uma variável de ambiente chamada `OPENSIGN_<GROUP>_<FIELD>`, por exemplo `OPENSIGN_EMAIL_API_KEY` ou `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR=true`. Um campo fixado fica bloqueado no navegador e exibe "Set by an environment variable." Isso é útil para infraestrutura como código e para manter um segredo fora do banco de dados por completo. Veja [Configurações de admin](settings.md#pinning-settings) para a lista completa de nomes.
