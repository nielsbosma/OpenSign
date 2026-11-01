# Início rápido

Esta página leva você do zero a um documento assinado na sua própria máquina. Para uma instalação de produção, leia [Implantação](deployment.md) em seguida.

## 1. Execute o OpenSign localmente

Você precisa do Docker e de uma cópia do repositório.

```sh
git clone https://github.com/nielsbosma/OpenSign.git
cd OpenSign
docker compose up -d --wait postgres s3 mailpit converter
docker compose run --rm s3-init
docker compose --profile prod up -d --build
```

Os dois primeiros comandos iniciam o Postgres, um armazenamento compatível com S3, o Gotenberg para arquivos Word e o Mailpit, e criam os buckets. O último compila e inicia a imagem de produção.

- OpenSign: http://localhost:8090
- Mailpit, uma caixa de entrada local para desenvolvimento: http://localhost:8026

A imagem de produção envia e-mails de verdade, portanto precisa de uma conta de e-mail real (passo 3). Para experimentar o OpenSign sem uma, execute-o no modo de desenvolvimento, em que todos os e-mails chegam ao Mailpit:

```sh
pnpm install
pnpm infra:up
cp .env.example .env
pnpm dev            # web app on http://localhost:5173
```

## 2. Crie o primeiro admin

Uma instalação nova imprime um **código de configuração** no log, para que ninguém mais possa reivindicá-la antes de você.

```sh
docker compose logs app | grep "setup code"
```

No modo de desenvolvimento, ele aparece no terminal onde o `pnpm dev` está rodando.

Abra o OpenSign e informe o código de configuração, seu nome, sua organização, seu e-mail e uma senha. Veja [Configuração inicial e e-mail](setup.md) para saber o que cada passo faz.

## 3. Conecte o e-mail

O OpenSign envia convites, códigos e documentos assinados por e-mail, por isso pede um provedor de e-mail antes que alguém possa enviar. Informe uma chave de API do Resend e um endereço de remetente em um domínio que você verificou no Resend. O OpenSign envia um e-mail de teste e só continua depois que ele for entregue.

No modo de desenvolvimento, você pode deixar a chave vazia: o e-mail de teste e tudo o que vem depois vão para o Mailpit.

## 4. Envie um documento

1. Vá para **Documentos** e escolha **Novo documento**.
2. Solte um arquivo PDF ou Word, dê um título a ele e crie-o.
3. Em **Partes**, adicione você mesmo como signatário, com o seu endereço de e-mail.
4. Escolha a parte na barra de ferramentas de campos e posicione um campo **Assinatura** na página.
5. Escolha **Enviar para assinatura**.

Abra o convite, siga o link, desenhe ou digite uma assinatura e assine. Alguns segundos depois, o documento está **Assinado**, e você pode baixar o PDF selado na página do documento.

## 5. Confira o resultado

Abra **Verificar um documento** na barra lateral (ou acesse `/verify`) e solte o PDF selado. O OpenSign confere o selo e os carimbos de tempo e confirma que selou exatamente este arquivo. Um leitor de PDF que entende assinaturas, como o Adobe Acrobat, também mostra o selo.

## Próximos passos

- [Implantação](deployment.md): coloque em produção de verdade, no Sliplane ou em qualquer lugar que execute contêineres.
- [Identificação](identification.md): configure códigos por SMS, BankID ou Freja.
- [Usuários e funções](users-and-roles.md): convide seus colegas.
- [API REST](api.md): envie documentos a partir dos seus próprios sistemas.
