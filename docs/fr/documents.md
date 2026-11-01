# Documents

Dans OpenSign, un document est composé d’un ou plusieurs fichiers à signer, des parties qui y participent et des champs que chaque partie remplit. Vous le préparez en brouillon, vous l’envoyez, et OpenSign le prend en charge jusqu’à ce qu’il soit signé, refusé, annulé ou expiré.

## La vie d’un document

| Statut | Signification |
| --- | --- |
| **Brouillon** | Vous le préparez encore. Les fichiers, les parties et les champs peuvent changer. |
| **En attente** | Envoyé et en attente de signatures. Ses fichiers sont verrouillés dans un seul PDF qui ne peut plus changer. |
| **Signé** | Toutes les personnes qui devaient agir ont signé ou approuvé. Le PDF scellé est prêt. |
| **Refusé** | Une partie a refusé. La signature est arrêtée pour tout le monde. |
| **Annulé** | L’expéditeur (ou un gestionnaire) l’a arrêté. |
| **Expiré** | La date limite est passée avant que tout le monde ait signé. |

Un document envoyé ne peut pas être modifié. Pour changer un document en attente, annulez-le et démarrez un nouveau brouillon.

## Créer un document

Choisissez **Nouveau document** sur la page **Documents**. Déposez un ou plusieurs fichiers, ou partez d’un [modèle](templates.md), et donnez un titre au document.

### Fichiers

- Les fichiers **PDF** sont acceptés tels quels, après nettoyage (voir ci-dessous).
- Les fichiers **Word** (`.docx`, `.doc`), **OpenDocument** (`.odt`) et **RTF** sont convertis en PDF par Gotenberg. Cela suppose un convertisseur sur l’installation ; sans lui, seuls les PDF sont acceptés.
- Un document peut avoir plusieurs fichiers. À l’envoi, ils sont combinés dans l’ordre en un seul PDF.
- La taille maximale d’un envoi est définie par un admin dans **Paramètres → Sécurité** (25 Mo par défaut).

Les balises de texte et les champs de formulaire remplissables présents dans le fichier deviennent des champs à l’import. OpenSign indique combien il en a ajouté et quelles balises il n’a pas pu utiliser. Voir [Balises de texte, import de formulaires et détection de champs](text-tags.md).

Chaque PDF est contrôlé d’après son contenu, pas son nom, et réécrit sans JavaScript, actions de formulaire, fichiers incorporés, XFA ni actions de lancement ou d’URI. Si quelque chose a été retiré, OpenSign vous le dit. Personne, signataires compris, ne voit jamais le contenu actif d’origine.

### Détails

| Paramètre | |
| --- | --- |
| Titre | Affiché dans la liste des documents, l’invitation et la page de signature. |
| Message aux parties | Affiché dans l’invitation et sur la page de signature. |
| Langue des signataires | La langue des e-mails, des SMS et de la page de signature, sauf si une partie a la sienne. Vide, la valeur par défaut de **Paramètres → Organisation** s’applique. Voir [La page de signature](signing-page.md#languages). |
| Expire | La date après laquelle le document ne peut plus être signé. Il passe alors à **Expiré**. |
| Relancer tous les (jours) | Envoie une relance aux parties qui n’ont pas encore signé, tous les tant de jours (jusqu’à 60). 0 désactive les relances. |
| Votre référence | Un identifiant externe, tel qu’un numéro de CRM ou de dossier. Vous pouvez le rechercher dans la liste des documents, et il figure dans les webhooks et l’API. |

### Parties

Ajoutez toutes les personnes qui signent, approuvent ou reçoivent une copie. Pour chaque partie, vous choisissez son rôle, la manière dont son lien lui parvient et la façon dont elle doit s’identifier. **Signer dans l’ordre**, sur la même carte, les invite une étape à la fois. Voir [Parties, rôles et ordre de signature](parties.md) et [Identification](identification.md).

### Champs

Choisissez une partie dans la barre d’outils des champs, puis placez des champs sur les pages. Les champs peuvent aussi venir du fichier lui-même : les [balises de texte](text-tags.md#text-tags) comme `{{Sign;type=signature;role=Tenant}}` et les champs d’un [formulaire PDF remplissable](text-tags.md#pdf-forms) deviennent des champs à l’import, et [Détecter les champs](text-tags.md#detect-fields), dans la barre d’outils, suggère des champs là où la page comporte des lignes et des cases à remplir. Chaque champ appartient à une partie, qui le remplit en signant. Sélectionnez un champ pour le modifier dans le panneau **Champ** : qui le remplit, son libellé, s’il est obligatoire, et les réglages ci-dessous.

| Champ | |
| --- | --- |
| Signature | Une signature dessinée ou saisie. |
| Initiales | Les initiales de la partie. |
| Texte | Du texte libre. Peut avoir un motif, une longueur et une valeur par défaut. |
| Nombre | Un nombre, avec une valeur minimale et maximale et un [format](#number-and-date-formats). |
| Liste déroulante | Un choix dans une liste d’options. |
| Choix multiple | Un nombre quelconque de choix dans une liste, écrits séparés par des virgules sur la page. |
| Bouton radio | Un groupe de boutons dont la partie en choisit un. Voir [Groupes de boutons radio](#radio-groups). |
| Case à cocher | Peut être obligatoire, ou cochée par défaut. |
| Téléphone | Un numéro de téléphone avec son indicatif de pays, enregistré sous la forme +46701234567. |
| Sélecteur de date | Une date que la partie choisit dans un calendrier, avec une date au plus tôt et au plus tard et un [format](#number-and-date-formats). |
| Image | La partie importe une image ou prend une photo. Elle est dessinée dans le cadre, mise à l’échelle pour s’y ajuster. |
| Fichier | La partie importe un PDF, un JPEG ou un PNG directement sur la page. Son nom figure dans le cadre et le fichier est scellé dans le PDF signé, comme les [pièces jointes](attachments.md). |
| Formule | Un nombre calculé à partir d’autres champs, comme un total avec TVA. Voir [Formules](#formulas). |
| Date | Remplie avec la date de signature, dans le format que vous choisissez. |
| Nom, E-mail, Entreprise | Remplis à partir des informations de la partie. |

Choisissez **Moi, maintenant (prérempli)** dans la barre d’outils pour placer une valeur que vous renseignez vous-même avant l’envoi, comme un numéro de contrat, un montant ou une date. Les champs obligatoires de ce type doivent être remplis avant que le document puisse être envoyé.

Les champs texte, nombre, liste, radio, case à cocher, téléphone et date d’une partie peuvent avoir une **Valeur par défaut** dont ils partent. Activez **Lecture seule** et la partie la voit sans pouvoir la modifier.

#### Contrôles et textes d’erreur

Les contrôles s’exécutent sur la page de signature pendant que la partie saisit, puis de nouveau sur le serveur au moment de la signature : une valeur qui enfreint un contrôle ne peut donc jamais arriver dans le PDF scellé.

| Contrôle | S’applique à | |
| --- | --- | --- |
| Format autorisé | Texte, initiales, entreprise | Un préréglage (adresse e-mail, chiffres uniquement, code postal suédois, numéro personnel ou numéro d’organisation) ou une expression régulière de votre choix. La valeur entière doit correspondre. |
| Longueur minimale, Longueur maximale | Texte, initiales, entreprise | En caractères. |
| Minimum, Maximum | Nombre | Bornes incluses. |
| Au plus tôt, Au plus tard | Sélecteur de date | Bornes incluses. |
| Obligatoire | Chaque champ que la partie remplit | |

**Texte d’erreur** remplace l’erreur intégrée par la vôtre, pour tout contrôle de ce champ, par exemple *Écrivez le numéro de commande sous la forme AB1234*. Sans lui, la partie voit une erreur intégrée dans sa propre langue.

Les motifs utilisent la syntaxe courante des expressions régulières : classes comme `[0-9]` et `\d`, répétitions comme `{3}`, `+` et `?`, alternatives avec `|` et groupes. Les assertions avant et arrière et les références arrière ne sont pas prises en charge, ce qui garde chaque contrôle rapide.

#### Formats de nombre et de date

Le format détermine comment une valeur est écrite sur la page ; ce qui est enregistré reste simple (`1234.5`, `2026-12-31`), de sorte que les exports et l’API reçoivent toujours la même forme.

Écrivez un format de nombre tel que 1234 y apparaîtrait. `1,234.00` donne une virgule entre les milliers et deux décimales, `1 234,00 kr` une espace, une virgule décimale et une devise après, et `$1234` aucune décimale avec un signe dollar. Les valeurs sont arrondies à la demi-unité supérieure selon les décimales du format. Les parties peuvent saisir les nombres des deux façons, par exemple `1 234,5` ou `1,234.50`.

Les dates peuvent s’écrire AAAA-MM-JJ (YYYY-MM-DD), DD/MM/YYYY, MM/DD/YYYY, DD.MM.YYYY ou D MMM YYYY (31 Dec 2026). Les mêmes formats s’appliquent à la date de signature.

#### Groupes de boutons radio

Placer un champ **Bouton radio** ajoute un bouton. Sélectionnez-le et choisissez **Ajouter un bouton au groupe** pour chaque option supplémentaire, puis faites glisser les boutons à leur place. Chaque bouton représente une option ; le groupe partage son libellé et le fait qu’une réponse soit obligatoire. La partie peut choisir un seul bouton du groupe.

#### Champs conditionnels

Sous **Afficher Seulement Si**, dans le panneau Champ, ajoutez des conditions sur d’autres champs. Le champ n’est affiché que tant que toutes sont remplies, par exemple *Numéro d’organisation* seulement quand *Type* vaut *Entreprise*, ou une zone *Motif* seulement quand la case *Autre* est cochée. Les champs soumis à des conditions sont dessinés avec une bordure en pointillés dans l’éditeur.

| Règle | |
| --- | --- |
| Est, N’est Pas | Compare avec une option, un nombre ou un texte. Le texte est comparé sans tenir compte de la casse, et les nombres par leur valeur : 10 et 10.0 sont donc identiques. |
| Est Rempli, Est Vide | Pour une case à cocher : Est Cochée, N’est Pas Cochée. |
| Contient | Une liste à choix multiple a l’option cochée, ou un texte contient les mots. |
| Est Supérieur à, Est Inférieur à | Les nombres, et les dates (Est Après, Est Avant). |

Un champ masqué n’est jamais obligatoire, reste vide quand la partie signe et n’est pas écrit sur le PDF signé. Un champ qui dépend d’un champ masqué le voit comme vide : le masquage se propage donc en cascade.

Les conditions désignent les champs par leur libellé : le champ que vous consultez a donc besoin d’un libellé, et un seul champ peut le porter. Renommer un champ dans l’éditeur met à jour les conditions et les formules qui l’utilisent. Le champ d’une partie peut consulter ses propres champs, les valeurs que vous avez préremplies, et les champs des parties qui signent avant elle quand **Signer dans l’ordre** est activé. Le serveur vérifie de nouveau les conditions quand la partie signe : une partie ne peut donc pas sauter un champ en modifiant la page.

#### Formules

Un champ **Formule** calcule un nombre à partir d’autres champs pendant que la partie remplit le document. La partie voit le résultat mais ne peut pas le modifier, et le serveur le recalcule à partir des valeurs soumises quand elle signe : le résultat enregistré est donc fiable. Donnez-lui un [format de nombre](#number-and-date-formats) pour l’arrondir et l’écrire, par exemple `1 234,00 kr`.

Écrivez les libellés des autres champs entre accolades, et utilisez `+ - * /` (ou `× ÷`), des parenthèses et ces fonctions :

| Fonction | |
| --- | --- |
| `round(x, 2)` | Arrondit à la demi-unité supérieure avec 0 à 6 décimales ; sans second argument, à un nombre entier. |
| `floor(x)`, `ceil(x)`, `abs(x)` | Arrondi vers le bas, vers le haut, ou valeur sans son signe. |
| `min(a, b, …)`, `max(a, b, …)`, `sum(a, b, …)` | |
| `if(condition, then, else)` | Les conditions comparent avec `=`, `!=`, `<`, `<=`, `>` et `>=`, et se combinent avec `and(…)`, `or(…)` et `not(…)`. |
| `contains(list, "text")` | Indique si une liste à choix multiple a l’option cochée, ou si un texte contient les mots. |

Exemples :

- La TVA sur un prix : `{Price} * 0.25`
- Un total avec TVA : `round({Hours} * {Rate} * 1.25, 2)`
- Une remise au-delà d’un montant : `if({Amount} >= 10000, {Amount} * 0.05, 0)`
- Un prix selon l’offre : `if({Plan} = "Pro", 99, 49) * {Seats}`

Un champ vide compte pour 0, une case cochée pour 1. Une formule qui divise par zéro, ou qui utilise un texte qui n’est pas un nombre, reste vide. Une formule que vous remplissez vous-même ne peut utiliser que vos propres champs préremplis et est calculée à l’enregistrement ; la formule d’une partie peut utiliser les mêmes champs qu’une [condition](#conditional-fields).

Les champs sont enregistrés sous forme de fractions de la page : ils restent donc en place quelle que soit la taille de la page.

## Avant l’envoi

Le panneau **Analyse** vérifie la configuration du document au fil de votre travail et liste tout ce qui empêcherait de le signer ou affaiblirait le résultat :

- Les erreurs, qui bloquent l’envoi : aucun fichier, aucun signataire ni approbateur, une partie qui reçoit le lien par e-mail ou SMS mais n’a pas d’adresse e-mail ou de numéro de téléphone, une partie qui doit confirmer avec un code SMS mais n’a pas de numéro de téléphone, un lecteur avec des champs à remplir, un champ obligatoire de l’expéditeur resté vide.
- Les avertissements, à examiner : un signataire sans champ de signature (sa signature n’apparaît alors que sur la page de preuve), une partie eID sans numéro d’identification personnel (n’importe qui possédant le lien et un eID pourrait alors signer), une partie sans e-mail ni téléphone (elle ne recevra donc pas la copie signée).

Si un admin a configuré l’IA, le même panneau peut aussi demander à une IA de lire le document. Voir [Assistant IA et analyse](ai.md#document-review).

## Aperçu

**Aperçu**, à côté de **Envoyer pour signature**, montre la page de signature telle qu’une partie donnée la verra, avec ses champs, sa langue et ses étapes d’identification. Voir [La page de signature](signing-page.md#previewing-as-a-party).

## Analyse IA par les signataires

Par défaut, les signataires peuvent confier à leur propre assistant IA une copie en lecture seule du document pour qu’il le leur explique avant de signer. Désactivez **Autoriser les signataires à partager avec leur IA** sous **Analyse IA par les signataires** pour l’empêcher. Voir [La page de signature](signing-page.md#review-with-your-ai).

## Envoi

Choisissez **Envoyer pour signature**. OpenSign combine les fichiers en un seul PDF, le verrouille et notifie les premières parties. Pour les parties dont la livraison est **Lien**, OpenSign affiche **Remettre les liens de signature** avec chaque lien à copier. Vous pouvez obtenir un nouveau lien plus tard depuis la carte de la partie, et **Nouveau lien** révoque d’abord l’ancien.

## Tant qu’il est en attente

Depuis la page du document, vous pouvez :

- **Relancer** toutes les personnes dont c’est le tour et qui n’ont pas encore signé.
- Obtenir ou remplacer le lien de signature d’une partie.
- Démarrer **Signer en personne** pour une partie qui signe sur votre appareil. Voir [Signature en personne](in-person.md).
- **Annuler** le document, avec un motif facultatif. Tous les liens de signature cessent de fonctionner et les parties déjà notifiées en sont informées.
- Consulter l’**Activité** : chaque étape du journal d’audit du document, comme qui l’a ouvert, s’est identifié, a signé ou l’a téléchargé.

## Quand il est terminé

Une fois la dernière partie signée, OpenSign scelle le document en arrière-plan, en général en quelques secondes. La page du document propose alors le PDF scellé et l’original tel qu’envoyé. Les parties qui ont une adresse e-mail reçoivent aussi le PDF scellé. Voir [PDF scellés et preuves](sealed-pdfs.md).

Chaque téléchargement est consigné dans le journal d’audit.

Pour récupérer sous forme de données ce que les parties ont renseigné, choisissez **Exporter les Données** dans le menu **⋯**. Voir [Exporter les données saisies](data-export.md).

## Partager avec des collègues

Les documents appartiennent à la personne qui les a créés. Pour qu’un collègue puisse en voir un, partagez-le avec lui, et laissez-le éventuellement le modifier. Les gestionnaires, les auditeurs et les admins voient tous les documents sans partage. Voir [Utilisateurs et rôles](users-and-roles.md).

## Suppression

Un document qui n’est pas en attente peut être supprimé avec ses fichiers. Annulez d’abord un document en attente. Les documents signés ne peuvent être supprimés que par une personne qui gère tous les documents, et un document sous conservation légale ne peut être supprimé par personne. La suppression est consignée dans le journal d’audit. Les documents peuvent aussi être supprimés automatiquement par des règles de conservation. Voir [Conservation et conservation légale](retention.md).

## Retrouver des documents

La page **Documents** liste les documents que vous pouvez voir, du plus récent au plus ancien, avec un onglet par statut. Recherchez par titre ou par votre référence.

Classez les documents dans des dossiers depuis la colonne de dossiers à côté de la liste, et archivez ceux qui sont terminés pour qu’ils quittent les onglets de statut. L’onglet **Archivé** les liste. Voir [Dossiers et archive](folders.md).
