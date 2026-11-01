# Modèles

Un modèle est un document que vous préparez une fois et envoyez plusieurs fois : un contrat de travail, un bail, un formulaire de consentement. Il a les mêmes fichiers, parties, champs, message et paramètres qu’un document, mais n’est jamais envoyé lui-même. Chaque fois que vous l’utilisez, OpenSign en crée un nouveau brouillon.

## Qui peut utiliser les modèles

- Toute personne qui peut envoyer des documents voit tous les modèles et peut démarrer un document à partir de l’un d’eux.
- Créer et modifier des modèles demande l’autorisation **templates.manage**, dont disposent les gestionnaires et les admins. Voir [Utilisateurs et rôles](users-and-roles.md).

## Créer un modèle

1. Allez dans **Modèles** et choisissez **Nouveau modèle**. OpenSign crée un modèle sans titre et l’ouvre.
2. Donnez-lui un titre, importez les fichiers et définissez le message, la langue, l’ordre de signature et les relances comme pour un document.
3. Ajoutez les parties. Pour un modèle, ce sont généralement des espaces réservés, comme « Locataire » ou « Employé », avec le rôle, la livraison et l’identification voulus. La personne qui utilise le modèle renseigne les vrais noms et coordonnées.
4. Placez les champs de chaque partie. Les [balises de texte](text-tags.md) du fichier peuvent les placer pour vous : le `role` d’une balise nomme la partie réservée, et un rôle que le modèle n’a pas encore est ajouté.

Un modèle reste un modèle ; il n’y a pas d’étape « envoyer ».

## Utiliser un modèle

- **Un document** : choisissez **Nouveau document**, sélectionnez le modèle et donnez un titre au document. OpenSign copie les fichiers, les parties (avec leurs demandes de pièces jointes), les champs, le message, la langue, l’ordre de signature et les relances dans un nouveau brouillon. Renseignez les vrais noms et coordonnées, puis envoyez-le comme d’habitude.
- **Plusieurs documents** : utilisez l’[envoi groupé](bulk-send.md) pour envoyer le modèle à chaque ligne d’une feuille de calcul.
- **Laisser les gens le démarrer eux-mêmes** : activez un [lien de partage](share-links.md). Toute personne disposant du lien ou de son code QR confirme son e-mail et signe sa propre copie.
- **Depuis l’API** : `POST /api/v1/documents` avec `templateId`, puis définissez les parties et envoyez. Le point d’accès `newfromtemplate` de Scrive fait la même chose. Voir [API REST](api.md).

Pour rassembler ce que les gens ont renseigné dans chaque document créé à partir d’un modèle, choisissez **Exporter les Données** dans le menu **⋯** du modèle. Voir [Exporter les données saisies](data-export.md).

Un document créé à partir d’un modèle est indépendant de celui-ci : modifier le modèle plus tard ne change pas les documents déjà créés. Les fichiers du modèle sont partagés, pas copiés, et n’occupent donc pas d’espace de stockage supplémentaire.

Chaque document indique sous son titre de quel modèle et de quelle version il provient, par exemple « Issu de Lease, version 3 ».

## Versions

OpenSign conserve un historique de chaque modèle. Une version contient tout ce qu’un document copie : le titre, le message, la langue, l’ordre de signature, les relances, le réglage d’analyse IA, les fichiers, les parties avec leurs demandes de pièces jointes, et chaque champ avec ses options, sa validation, son format, son groupe radio, son indicateur de lecture seule, ses conditions et sa formule.

- **Versions automatiques.** Quand quelqu’un utilise un modèle modifié depuis sa dernière version, OpenSign enregistre d’abord une nouvelle version. Ainsi, chaque document peut nommer la version exacte dont il provient, sans que personne ait à penser à l’enregistrer.
- **Enregistrer la version.** Ouvrez **Plus → Historique des versions** et enregistrez l’état actuel avec une note, par exemple « Nouveau préavis après la loi sur les loyers de 2027 ». Enregistrer de nouveau sans modification met seulement la note à jour.
- **L’en-tête** d’un modèle indique à quelle version il en est, ou « Modifié depuis la version 4 » quand il y a des modifications non enregistrées. Cliquez dessus pour ouvrir l’historique.

### Historique des versions

**Historique des versions** liste les versions de la plus récente à la plus ancienne, avec la personne qui les a enregistrées, la note et le nombre de documents créés à partir de chacune. Choisissez-en une pour voir :

- ce qui a changé par rapport à la version précédente : paramètres, fichiers, et parties et champs ajoutés, supprimés ou modifiés (par exemple « Champ Address pour Tenant : position, obligatoire modifié »),
- les documents créés à partir d’elle que vous pouvez voir,
- **Aperçu**, qui montre les pages de la version avec ses champs placés comme ils l’étaient.

Quand le modèle a des modifications non enregistrées, **Actuel**, en haut, montre ce qui a changé depuis la dernière version.

### Restaurer une version

**Restaurer** remet le modèle tel qu’il était dans cette version : fichiers, parties, champs et paramètres. Rien n’est perdu. Les modifications non enregistrées sont d’abord enregistrées comme une version, et la restauration devient elle-même une nouvelle version marquée « Restaurée depuis la version 2 ». Les documents déjà créés ne sont pas touchés.

Les numéros d’identification personnels des versions sont stockés chiffrés, comme sur les parties, et affichés masqués.

## Remplacer un fichier

Quand le texte du contrat change mais pas la mise en page, utilisez le bouton de remplacement à côté du fichier sous **Fichiers** plutôt que de le supprimer et de replacer les champs. Importez le nouveau fichier PDF ou Word et OpenSign :

- le met au même endroit parmi les fichiers du modèle,
- garde chaque champ où il était, et décale les champs des fichiers suivants si le nombre de pages a changé,
- déplace les champs qui se trouvaient sur des pages que le nouveau fichier n’a pas vers sa dernière page et les liste, pour que vous puissiez les corriger.

Si le nouveau fichier contient des [balises de texte](text-tags.md#text-tags), ce sont elles qui l’emportent : les champs de l’ancien fichier sont abandonnés et les balises sont placées à la place, en ajoutant les parties qu’elles nomment.

Sur un modèle, l’état antérieur au remplacement est conservé comme une version : l’ancien fichier reste donc dans l’historique et peut être restauré. Le remplacement fonctionne aussi sur les brouillons. Depuis l’API, `POST /api/v1/documents/{id}/files/{fileId}/replace` avec le fichier en `file` multipart. Voir [API REST](api.md#documents).

## Dupliquer un modèle

**Plus → Dupliquer** crée une copie nommée « Lease (copie) » dans le même dossier, avec tout ce que contient le modèle. La copie démarre son propre historique. Depuis l’API, `POST /api/v1/documents` avec `isTemplate: true` et l’original comme `templateId`.

## Dossiers et archive

Les dossiers de modèles gardent en ordre une longue liste de modèles, comme *RH* et *Ventes*. Archivez un modèle que vous n’utilisez plus : il disparaît de **Nouveau document** et de l’envoi groupé jusqu’à ce que vous le restauriez. Voir [Dossiers et archive](folders.md).

## Modèles et conservation

Les règles de conservation ne suppriment jamais les modèles, et leurs versions restent donc aussi. Supprimer un modèle supprime son historique, ainsi que les fichiers que seules ses versions utilisaient. Voir [Conservation et conservation légale](retention.md).
