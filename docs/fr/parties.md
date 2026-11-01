# Parties, rôles et ordre de signature

Une partie est toute personne qui participe à un document : quelqu’un qui signe, qui approuve, ou qui reçoit seulement la copie signée. Vous ajoutez les parties sur la page du document, sous **Parties**.

## Informations sur la partie

| Champ | |
| --- | --- |
| Nom | Obligatoire. Affiché sur la page de signature et dans les preuves. |
| Rôle | Signe, Approuve ou Reçoit une copie. Voir ci-dessous. |
| E-mail | Nécessaire pour la livraison par e-mail et pour recevoir la copie signée. |
| Mobile | Avec l’indicatif du pays, par exemple `+46 70 123 45 67`. Nécessaire pour la livraison par SMS et les codes SMS. |
| Entreprise | Facultatif. Remplit les champs Entreprise. |
| Langue | Ses e-mails, SMS et page de signature. **Comme le document** par défaut. Voir [La page de signature](signing-page.md#languages). |
| Envoyer le lien par | La manière dont la partie reçoit son lien de signature. Voir [Livraison](#delivery). |
| S’identifier pour consulter | Ce que la partie doit faire avant de pouvoir ouvrir le document. Voir [Identification](identification.md). |
| S’identifier pour signer | Ce que la partie doit faire avant de pouvoir signer. Voir [Identification](identification.md). |
| Demander une identification par webcam | La partie prend une photo d’elle-même avant de signer. Voir [Photo webcam](identification.md#webcam-photo). |
| Numéro d’identification personnel | Pour l’identification par eID. S’il est renseigné, seul l’eID de cette personne peut signer. Stocké chiffré et affiché masqué. |
| Ordre de signature | Quand le document se signe dans l’ordre, les numéros les plus bas passent en premier. Voir [Ordre de signature](#signing-order). |
| Demander des pièces jointes | Des fichiers que la partie doit ou peut importer. Voir [Pièces jointes des signataires](attachments.md). |

## Rôles

| Rôle | Ce que fait la partie |
| --- | --- |
| **Signe** (signataire) | Lit le document, remplit ses champs et signe avec une signature dessinée ou saisie. |
| **Approuve** (approbateur) | Lit le document et l’approuve. Pas d’image de signature, mais l’approbation figure dans les preuves comme une signature. |
| **Reçoit une copie** (lecteur) | N’agit pas. Reçoit le PDF scellé quand le document est terminé. Un lecteur ne peut pas avoir de champs. |

Le document est terminé quand chaque signataire et chaque approbateur a agi.

## Livraison

| Option | |
| --- | --- |
| **E-mail** | Par défaut. OpenSign envoie par e-mail une invitation avec le lien personnel, et des relances si elles sont activées. |
| **SMS** | OpenSign envoie le lien par SMS. Nécessite un fournisseur de SMS dans **Paramètres → SMS**. |
| **Lien (vous le remettez)** | OpenSign n’envoie rien. À l’envoi du document, il vous montre le lien à transmettre, par exemple dans votre propre e-mail ou messagerie, ou à afficher dans votre propre application avec la [signature intégrée](embedding.md). |
| **En personne (sur votre appareil)** | La partie signe sur votre ordinateur ou votre tablette, depuis la page du document. Voir [Signature en personne](in-person.md). |
| **Aucun** | OpenSign n’envoie rien et n’affiche aucun lien. Utile pour les lecteurs qui n’ont besoin que de la copie signée, envoyée à leur adresse e-mail. |

Les options qui ne sont pas configurées sur cette installation sont affichées mais ne peuvent pas être choisies tant qu’un admin ne les a pas configurées.

### Liens de signature

Chaque partie a un lien personnel. Il n’est valable que pour cette partie, et cesse de fonctionner quand :

- le document est terminé, refusé, annulé ou expire,
- la durée de vie propre au lien est écoulée : la date d’expiration du document, ou sinon le nombre de jours défini dans **Paramètres → Sécurité** (30 par défaut), compté à partir du moment où la partie a été notifiée pour la première fois, ou
- un nouveau lien est émis pour la partie, ce qui révoque l’ancien.

La partie secrète du lien se trouve après le `#`, de sorte qu’elle n’arrive jamais dans les journaux du serveur, et la page de signature la retire de la barre d’adresse. Voir [Modèle de sécurité](security.md#signing-links).

## Ordre de signature

Par défaut, tout le monde est invité en même temps et peut signer dans n’importe quel ordre. Activez **Signer dans l’ordre** sur la carte **Parties** pour inviter les parties une étape à la fois :

- Chaque signataire et approbateur affiche son numéro d’étape. Activer l’ordre part de l’ordre dans lequel les parties sont listées.
- Utilisez les flèches sur la carte d’une partie pour la monter ou la descendre.
- Pour que plusieurs parties signent ensemble à la même étape, donnez-leur le même numéro d’**Ordre de signature** sous **Modifier**. Les numéros les plus bas passent en premier.
- Quand toutes les parties d’une étape ont agi, OpenSign invite l’étape suivante.
- Une partie qui ouvre son lien avant son tour voit « D’autres personnes signent avant vous » et reçoit un e-mail quand c’est son tour.

Les lecteurs ne font pas partie de l’ordre. Ils reçoivent le PDF scellé à la fin.

## Refus

Un signataire ou un approbateur peut refuser, avec un motif facultatif. Refuser arrête le document pour tout le monde : il passe à **Refusé**, tous les liens cessent de fonctionner, l’expéditeur est prévenu et un webhook `document.declined` est émis avec le motif.

## Modifier les parties

Les parties peuvent être modifiées tant que le document est un brouillon. Retirer une partie retire aussi ses champs. Une fois le document envoyé, ses parties sont figées ; pour les changer, annulez le document et démarrez-en un nouveau.
