# La page de signature

C’est ce que voient les personnes à qui vous envoyez des documents. Elles n’ont besoin ni de compte ni d’application, seulement d’un navigateur. La page fonctionne sur téléphone, tablette et ordinateur, en mode clair et sombre.

## Ouvrir le lien

L’invitation arrive par e-mail ou par SMS, ou directement de votre part. Elle nomme votre organisation et la personne qui a envoyé le document, et indique jusqu’à quand le lien fonctionne.

Si la partie doit s’identifier pour consulter le document, elle le fait d’abord : avec un code SMS, BankID, Freja ou un autre eID. Voir [Identification](identification.md). Sinon, le document s’ouvre aussitôt.

Un lien incomplet, révoqué, expiré, ou qui appartient à un document qui n’est plus en attente affiche un message clair à la place du document.

## Lire et remplir

La page montre :

- de qui vient le document, et le message aux parties s’il y en a un,
- ce qu’on demande à la partie : signer, approuver ou recevoir une copie,
- le document entier, avec les champs de la partie mis en évidence.

La partie remplit ses champs : texte, nombres, listes déroulantes, listes à choix multiple, groupes de boutons radio, cases à cocher, numéros de téléphone, dates choisies dans un sélecteur de date, images et fichiers importés directement, initiales et signatures. Chaque champ est contrôlé pendant la saisie, avec des erreurs dans sa langue ou le [texte d’erreur que vous avez écrit](documents.md#checks-and-error-texts), et la partie ne peut pas signer tant que tous les champs ne sont pas valides. Les champs nom, e-mail et entreprise sont remplis à partir de ses informations. Si vous avez demandé des [pièces jointes](attachments.md), elle les importe ici. Si vous avez demandé une [photo webcam](identification.md#webcam-photo), elle la prend ici.

Dans un document qui se signe dans l’ordre, une partie qui ouvre son lien trop tôt voit « D’autres personnes signent avant vous. Nous vous enverrons un e-mail quand ce sera votre tour. »

## Signer

Une signature est soit :

- **Dessiner** : avec un doigt, un stylet ou la souris, soit
- **Saisir** : la partie tape son nom, qui s’affiche dans un style de signature, soit
- **Enregistrée** : une signature enregistrée précédemment, proposée en premier s’il y en a une. Les utilisateurs d’OpenSign qui signent leurs propres documents obtiennent celle de leur [compte](account-security.md#saved-signature). Les autres signataires peuvent cocher **Mémoriser ma signature dans ce navigateur** en signant ; la prochaine fois qu’une partie portant le même nom signe dans ce navigateur, elle lui est proposée ici, avec **Oublier** pour la retirer. Elle reste dans le navigateur et n’est envoyée nulle part tant que la personne ne signe pas avec elle. Rien n’est mémorisé pendant une signature en personne sur l’appareil de quelqu’un d’autre.

Avant de signer, la partie confirme : « J’ai lu le document et j’accepte de le signer électroniquement. Cela m’engage autant qu’une signature sur papier. » Si la partie doit s’identifier pour signer, elle le fait maintenant. Puis elle choisit **Signer** (ou **Approuver** pour les approbateurs).

La partie voit une confirmation. Quand tout le monde a signé, les parties qui ont une adresse e-mail reçoivent le PDF scellé par e-mail, et la page de signature propose **Télécharger le document signé** tant que le lien est valable.

## Refuser

**Refuser** demande un motif facultatif et arrête le document pour tout le monde. L’expéditeur est prévenu, ainsi que chaque partie qui avait déjà un lien.

## Langues

Les signataires peuvent être sollicités en onze langues : anglais, chinois (simplifié), hindi, espagnol, français, arabe, bengali, portugais, russe, ourdou et suédois. L’arabe et l’ourdou s’affichent de droite à gauche.

La langue de chaque partie détermine ses e-mails d’invitation et de relance, ses SMS et sa page de signature. C’est la **Langue** de la partie si vous en avez défini une, sinon la **Langue des signataires** du document, sinon la valeur par défaut de **Paramètres → Organisation**.

La page de signature a un sélecteur de langue dans son en-tête. Le choix d’un signataire ne dure que pour cet onglet du navigateur, et ne se transmet donc pas à la personne suivante sur un appareil partagé.

Certains textes restent en anglais ou en suédois : les messages propres à BankID, les messages d’erreur du serveur et les pages de preuve du PDF scellé.

## Analyse avec votre IA

Les signataires peuvent demander à leur propre assistant IA, comme ChatGPT, Claude ou Copilot, d’expliquer le document avant de signer. Sous **Analyser avec votre IA**, ils choisissent **Créer un lien d’analyse**, puis copient une invite toute prête ou choisissent **Ouvrir dans ChatGPT**, **Claude** ou **Copilot**, ce qui ouvre l’assistant avec l’invite déjà remplie.

Le lien d’analyse n’est pas le lien de signature :

- Il est en lecture seule. Il donne à l’assistant le texte du document et le PDF tel qu’envoyé, et ne peut ni signer, ni refuser, ni rien modifier.
- Il n’est affiché qu’une fois, et demande la même identification que pour consulter le document.
- Il expire après le nombre de jours défini dans **Paramètres → Sécurité** (7 par défaut), ou plus tôt à l’échéance du document ou à l’expiration du lien de signature, et autorise 100 récupérations au plus.
- Il cesse de fonctionner quand le signataire le révoque, quand vous émettez un nouveau lien de signature, ou quand le document n’est plus en attente.
- Chaque utilisation figure dans le journal d’audit.

Le texte est signalé à l’assistant comme non fiable, et les pages contenant du texte caché, comme du texte blanc ou invisible, sont signalées afin que l’assistant puisse en avertir le signataire.

Vous pouvez désactiver cela document par document sous **Analyse IA par les signataires** dans l’éditeur de document, ce qui révoque aussi les liens déjà partagés. Un admin peut le désactiver pour tout le monde en réglant le nombre de jours sur 0.

## Prévisualiser comme une partie

Choisissez **Aperçu** dans l’éditeur de document pour voir la page de signature telle qu’une partie donnée la verra : votre image de marque, le message, sa langue, ses champs, les demandes de pièces jointes et les étapes d’identification. Rien ne peut être signé ni envoyé depuis l’aperçu, et il n’est pas consigné comme l’ouverture du document par la partie.

## Image de marque

La page affiche le nom et le logo de votre organisation et utilise la couleur de votre marque, le tout depuis **Paramètres → Organisation**. Voir [Logo](settings.md#logo).

La page peut aussi être affichée dans votre propre application. Voir [Signature intégrée](embedding.md).

## Confidentialité et sécurité

- La partie secrète du lien se trouve après le `#`, de sorte qu’elle n’est pas envoyée au serveur dans les requêtes ni les journaux, et la page la retire de la barre d’adresse une fois chargée.
- Les actions d’un signataire n’utilisent jamais une session OpenSign connectée, même si le signataire en a une dans le même navigateur.
- Chaque étape est consignée dans un journal d’audit infalsifiable, comme la page l’indique au signataire.
