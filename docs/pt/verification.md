# Verificar um documento

Qualquer pessoa que tenha um PDF assinado pode conferir se ele é genuíno e não foi alterado. Não é necessária uma conta.

## Na página de verificação

Abra `https://<your-install>/verify` (também com link como **Verificar um documento** na barra lateral do aplicativo e impresso nas páginas de evidência de todo documento selado) e solte o PDF.

O arquivo é verificado no servidor e não é armazenado. A página responde a duas perguntas:

1. **Esta instalação selou exatamente este arquivo?** "Selado por este OpenSign", com o título do documento e o horário de conclusão, significa que o SHA-256 do arquivo corresponde a um documento que este servidor selou e que o seu selo é válido, feito com um dos certificados de selo deste servidor. "Não é um documento selado por este servidor" significa que ele foi alterado depois de selado ou vem de outro lugar.
2. **As assinaturas estão íntegras?** Cada assinatura e carimbo de tempo do arquivo é listado com quem o fez, quando e se está íntegro. Íntegro significa apenas que os bytes estão como foram assinados: qualquer pessoa pode criar um certificado com qualquer nome. Uma assinatura que está íntegra, mas não foi feita por este servidor, informa isso, e o nome nela não é confirmado.

## O que a verificação cobre

Para cada assinatura e carimbo de tempo de documento no arquivo, o OpenSign confere:

- se a criptografia da assinatura é válida e cobre os bytes que afirma cobrir,
- se ela cobre o arquivo inteiro ou apenas uma revisão anterior,
- se tudo o que foi acrescentado depois dela são apenas dados de validação: um armazenamento de segurança do documento com certificados e respostas de revogação, e carimbos de tempo de documento invisíveis,
- a própria assinatura da autoridade de carimbo de tempo (RSA de 2048 bits ou mais, ECDSA P-256 e P-384, SHA-2), se o certificado dela se destina a carimbo de tempo e se era válido no momento carimbado. Um carimbo de tempo que falha nessas verificações não mostra horário.

Qualquer outra alteração depois de uma assinatura, como uma nova anotação, uma edição de formulário, uma entrada de catálogo alterada, um objeto excluído ou um objeto novo que nada parece usar, marca aquela assinatura como não íntegra. O mesmo vale para uma atualização que não é escrita da maneira simples como o OpenSign escreve as suas. Isso fecha o truque comum de alterar um PDF assinado com uma atualização incremental que alguns leitores não sinalizam, e significa que um arquivo ao qual outra ferramenta acrescentou dados de validação pode aparecer como alterado.

O OpenSign não confere signatários nem autoridades de carimbo de tempo em uma lista de confiança. Para isso, use um leitor de PDF que tenha uma.

## Em um leitor de PDF

Os documentos selados são arquivos PAdES padrão, portanto o Adobe Acrobat, o Foxit e outros leitores que entendem assinaturas mostram o selo, o carimbo de tempo e se o documento mudou. Como o certificado do selo é autoassinado por padrão, um leitor pode dizer que não conhece a identidade do signatário; a página de verificação acima é a conferência de referência para a sua instalação. Veja [PDFs selados e evidências](sealed-pdfs.md#the-seal-certificate).

## Pela API

`POST /api/v1/verify` com o PDF bruto como corpo (até 40 MB) retorna o mesmo resultado em JSON. Não exige chave de API e tem limite de requisições por endereço. Veja [API REST](api.md).

```sh
curl -X POST https://sign.example.com/api/v1/verify \
  -H "Content-Type: application/pdf" --data-binary @signed.pdf
```
