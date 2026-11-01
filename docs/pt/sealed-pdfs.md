# PDFs selados e evidências

Quando a última parte assina, o OpenSign transforma o documento em um único PDF selado. Ele contém o documento com todos os valores e assinaturas carimbados, páginas de evidência que contam a história da assinatura e um selo digital que mostra que o arquivo não mudou desde então. Qualquer pessoa pode conferi-lo com um leitor de PDF comum ou na [página de verificação](verification.md), e ele continua verificável por anos, offline.

## O que o PDF selado contém

1. **O documento**, exatamente como foi enviado, com os valores dos campos e as assinaturas de cada parte carimbados no lugar.
2. **Páginas de evidência** no final:
   - o seu [logotipo](settings.md#logo), se você enviou um, o título do documento, o id dele e a sua organização,
   - cada parte: nome, e-mail, função, como se identificou (por exemplo, "BankID sueco, número de identificação pessoal 19xxxxxx-1234"), quando assinou e o endereço IP,
   - os eventos da trilha de auditoria: enviado, aberto, identificado, assinado e assim por diante, com os horários,
   - os anexos enviados pelos signatários e quaisquer [fotos da webcam](identification.md#webcam-photo), com os respectivos hashes SHA-256,
   - o hash SHA-256 do PDF original e onde verificar o documento.
3. **Arquivos incorporados**: um anexo de evidência em JSON com os mesmos fatos em formato legível por máquina, e os [anexos dos signatários](attachments.md) e as fotos da webcam.
4. **O selo**: uma assinatura PAdES sobre o arquivo inteiro em nome da sua organização, com um carimbo de tempo RFC 3161.
5. **Dados de validação de longo prazo** e um carimbo de tempo de arquivamento (veja abaixo).

## PAdES B-LTA

O selo segue o PAdES Baseline B-LTA, o nível do padrão europeu para documentos que precisam permanecer verificáveis por muito tempo:

- **B-B**: uma assinatura CAdES sobre o arquivo inteiro, feita pelo selo da organização.
- **B-T**: um carimbo de tempo RFC 3161 de uma autoridade de carimbo de tempo confiável, que comprova quando o selo foi feito.
- **B-LT**: todos os certificados dos quais o selo e o carimbo de tempo dependem, com respostas OCSP ou CRLs para cada um, armazenados no armazenamento de segurança do documento do PDF. O arquivo pode ser validado sem contatar ninguém, mesmo depois que os certificados expirarem.
- **B-LTA**: um carimbo de tempo do documento sobre tudo, incluindo os dados de validação.

O OpenSign confere cada resposta OCSP ou CRL antes de armazená-la: ela precisa ser assinada pelo emissor ou pelo seu respondedor delegado, referir-se ao certificado correto e estar atual. Os dados de validação e o carimbo de tempo de arquivamento são acrescentados como atualizações incrementais, de modo que os bytes cobertos pelo selo nunca mudam.

Se os dados de revogação não puderem ser obtidos, o documento é selado mesmo assim e a lacuna é registrada no log de auditoria. O mesmo vale para um certificado que um respondedor informa como revogado. Se a autoridade de carimbo de tempo não puder ser alcançada após várias tentativas, o documento é selado sem carimbo de tempo, em vez de ficar esperando.

## O certificado do selo

O OpenSign gera a chave do selo no servidor na primeira vez que sela um documento, como um certificado autoassinado em nome da sua organização, e a armazena criptografada. **Configurações → Assinatura e selo** mostra o assunto do certificado e por quanto tempo ele é válido.

**Substituir a chave do selo** cria um novo certificado autoassinado no nome atual da organização. Os novos documentos são selados com ele. Os documentos selados antes continuam válidos e verificáveis.

Como o certificado é autoassinado, os leitores de PDF mostram o selo como válido e inalterado, mas podem dizer que não conhecem a identidade do signatário até que você confie no certificado. A página de verificação confere que a sua instalação selou exatamente aquele arquivo.

## Configurações

Em **Configurações → Assinatura e selo**:

| Configuração | |
| --- | --- |
| Nome do selo | Exibido como o signatário do selo. Vazio usa o nome da organização. |
| Motivo da assinatura | O motivo registrado no selo. Padrão: "Signed with OpenSign". |
| Autoridade de carimbo de tempo (RFC 3161) | A URL da autoridade de carimbo de tempo. Padrão: `http://timestamp.digicert.com`. |

## Obter o PDF selado

- **Remetentes**: baixe-o na página do documento, que também oferece o original como foi enviado.
- **Partes**: todos que têm endereço de e-mail o recebem por e-mail quando o documento é concluído. Arquivos de até 10 MB são anexados; os maiores chegam como um link de download. A página de assinatura também oferece **Baixar documento assinado** enquanto o link é válido.
- **API**: `GET /api/v1/documents/{id}/file?version=signed`. Veja [API REST](api.md).
- **Webhooks**: `document.completed` traz o SHA-256 do arquivo selado. Veja [Webhooks](webhooks.md).

Cada download é registrado na trilha de auditoria.

## Autoridades de carimbo de data/hora confiáveis

Qualquer pessoa pode operar uma autoridade de carimbo de tempo, de modo que um carimbo de tempo conferido, por si só, apenas diz que o seu token é íntegro. Para que a página de verificação diga em quais carimbos de tempo você confia, liste as impressões digitais SHA-256 dos certificados deles em **Autoridades de carimbo de data/hora confiáveis**, em Configurações de admin → Assinatura e selo. A lista vem vazia por padrão.
