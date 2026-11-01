# Signature en personne

La signature en personne permet à une partie de signer sur votre propre ordinateur ou tablette, par exemple à un comptoir, lors d’une réunion ou chez un client. La partie n’a besoin ni d’adresse e-mail ni de téléphone.

## La configurer

Dans la boîte de dialogue de la partie, définissez **Envoyer le lien par** sur **En personne (sur votre appareil)**. OpenSign n’envoie alors rien à cette partie. Vous pouvez toujours lui demander de s’identifier avec un code SMS ou un eID ; elle le fait sur votre appareil.

## Signer

1. Envoyez le document comme d’habitude.
2. Quand c’est le tour de la partie, ouvrez le document et choisissez **Signer en personne** sur sa carte de partie.
3. OpenSign ouvre la page de signature avec un écran de passation : « Remettez l’appareil à … ». Il explique que tout ce que la personne fait est consigné dans les preuves du document.
4. La partie confirme avec **Je suis …**, lit le document, s’identifie si on le lui demande, et signe.
5. Quand elle a terminé, la page lui demande de vous rendre l’appareil.

## Ce qui est consigné

- Démarrer la signature en personne émet un nouveau lien pour la partie et retire tous les liens précédents : un lien démarré auparavant ne peut donc pas être réutilisé.
- Le journal d’audit consigne qui a démarré la session en personne et pour quelle partie. Les pages de preuve montrent l’identification, l’adresse IP et le navigateur de la partie, comme pour tout autre signataire.

## Signer soi-même un document

Pour un document que vous seul devez signer, comme une déclaration, une confirmation de commande ou un formulaire que vous remplissez pour vos propres archives, choisissez **Signer moi-même** sur le brouillon au lieu de **Envoyer**.

1. Choisissez comment vous vous identifiez : **Ma connexion à OpenSign** (par défaut), ou un eID tel que BankID quand un admin en a configuré un.
2. Choisissez **Signer maintenant**. Vous devenez le seul signataire : les signataires et approbateurs déjà présents sur le brouillon sont retirés et leurs champs deviennent les vôtres. Les personnes qui reçoivent une copie continuent d’en recevoir une.
3. OpenSign envoie le document et ouvre la page de signature sur votre appareil, sans écran de passation. Votre [signature enregistrée](account-security.md#saved-signature) vous est proposée si vous en avez une.
4. Quand vous signez, le document est scellé et vérifiable comme n’importe quel autre.

Avec **Ma connexion à OpenSign**, les pages de preuve indiquent « Identifié par : compte OpenSign connecté » avec votre nom et votre e-mail, et le journal d’audit consigne l’utilisateur qui l’a démarré. Si vous partez avant de signer, **Signer en personne** sur votre carte de partie rouvre la page de signature.

## Remarques

- **Signer en personne** n’est proposé que pour les parties dont la livraison est En personne, tant que le document est en attente et que c’est le tour de la partie.
- Démarrer une signature en personne, ou signer un document soi-même, demande un utilisateur connecté. Ni l’un ni l’autre n’est disponible avec une clé d’API ou via MCP.
- La livraison `pad` de Scrive correspond à la signature en personne. Voir [Compatibilité Scrive](scrive.md).
