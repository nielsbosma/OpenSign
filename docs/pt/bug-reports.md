# Relatar um erro

Encontrou algo que não funciona? Relate de dentro do OpenSign. O seu relato vira uma issue pública no GitHub, no seu próprio nome, para que quem desenvolve o OpenSign veja o que você viu.

## Como relatar

1. Abra o menu abaixo do seu nome, na parte de baixo da barra lateral, e escolha **Relatar um erro**.
2. Um painel se abre na parte de baixo da página. Escolha uma ferramenta:
   - **Comentário**: clique em um elemento, diga o que há de errado com ele e pressione Enter. Ele ganha uma caixa numerada.
   - **Ocultar**: arraste sobre qualquer coisa privada para cobri-la de preto.
   - **Pausar**: use a página normalmente, por exemplo para abrir antes um menu ou uma caixa de diálogo. Esc também pausa.
3. Descreva o que deu errado e o que você esperava. O título é opcional.
4. Pressione **Revisar o relatório**. Você vê a captura de tela final e cada linha que será enviada. Altere ou remova o que não quiser compartilhar, ou deixe a captura de fora.
5. Marque a caixa que confirma que o relato pode ser público e pressione **Continuar no GitHub**.
6. Uma nova janela se abre no serviço de relatos de erros. Ela mostra o relato mais uma vez. Entre com o GitHub ali e publique. A janela então leva à sua nova issue.

Você precisa de uma conta do GitHub com pelo menos uma semana. Se o navegador bloquear a nova janela, o OpenSign mostra um link para você abri-la.

## O que é publicado

A issue mostra:

- o seu título e descrição, e os seus comentários com o elemento a que cada um se refere,
- a captura de tela, se você a manteve,
- a página como um padrão, por exemplo `/documents/:id`, a versão do OpenSign, o seu navegador e sistema, o tamanho da janela e o idioma do app,
- títulos de caixas de diálogo abertas, erros recentes do console e requisições recentes que falharam, se você manteve essas linhas.

Ela nunca mostra o endereço do seu OpenSign, o endereço real da página, o seu nome ou e-mail no OpenSign, nem o conteúdo de um documento. As linhas técnicas perdem endereços de e-mail, endereços web, ids, tokens, chaves e números longos antes de saírem do OpenSign; cada um é substituído por `[removed]`.

A issue é criada pela sua conta do GitHub, com o selo do app **OpenDeploy Bug Reports**, que só pode abrir issues. O serviço usa o seu login para essa única requisição e depois o revoga.

## O que é coberto na captura de tela

Antes de a captura ser feita, o OpenSign cobre de cinza:

- tudo o que foi digitado em um campo,
- páginas e miniaturas de PDF,
- nomes e e-mails de partes e usuários,
- títulos de documentos,
- entradas da trilha de auditoria,
- endereços de webhook e endereços IP permitidos de chaves de API.

As caixas pretas que você desenha com **Ocultar** também ficam gravadas na imagem. Nada muda na própria página. Confira a captura na etapa de revisão: qualquer outra coisa privada cabe a você ocultar.

## Remover uma captura de tela

As capturas de tela são públicas e ficam guardadas até que alguém as remova. Toda issue termina com **Remover esta captura de tela**. Abra o link, entre com o GitHub, e a imagem é apagada e a issue informa isso. Você pode remover as suas próprias capturas; os mantenedores do repositório podem remover qualquer uma.

O GitHub mantém cópias em cache das imagens por um tempo, e qualquer pessoa pode tê-la salvo, então remover uma captura não consegue desfazer todas as cópias. Por isso a etapa de revisão pede que você confira se não há nada privado na tela.

## O que sai do seu OpenSign

Nada é enviado até você pressionar **Continuar no GitHub**. Então o servidor do seu OpenSign envia o relato ao serviço de relatos de erros em `sliplane.opendeploy.cloud`, que o guarda por no máximo uma hora até você publicá-lo. O OpenSign não guarda credenciais do GitHub.

Cada pessoa pode enviar 10 relatos por hora, e cada OpenSign 30 por hora.

## Para admins

Os relatos de erros vêm ativados por padrão. Em **Configurações → Relatos de erros** um admin pode desativá-los, o que oculta **Relatar um erro** para todos, ou enviar os relatos a outro repositório, como um fork. Veja [Configurações de admin](settings.md#bug-reports). Todo relato enviado é registrado na trilha de auditoria.

Uma instalação sem acesso de saída à internet não consegue enviar relatos.
