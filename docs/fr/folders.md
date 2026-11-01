# Dossiers et archive

Les dossiers regroupent les documents et les modèles, et l’archive met le travail terminé à l’écart. Les deux ne changent que l’endroit où les éléments sont listés. Ils ne changent jamais qui peut voir un document, comment il est conservé, ni ce qui lui arrive.

## Dossiers

Les pages **Documents** et **Modèles** ont une colonne de dossiers à gauche. **Tous les documents** (ou **Tous les modèles**) liste tout, quel que soit le dossier ; choisir un dossier liste ce qui s’y trouve directement. Sur les écrans étroits, les dossiers forment un menu au-dessus de la liste.

- Les documents et les modèles ont des dossiers séparés. Un document ne peut pas aller dans un dossier de modèles, ni l’inverse.
- Les dossiers s’imbriquent sur trois niveaux au plus, comme *Clients / Acme / 2026*.
- Les noms sont uniques au sein d’un dossier, sans tenir compte de la casse.
- Un document ou un modèle est dans un dossier au plus, ou dans aucun.

### Déplacer des éléments dans un dossier

- Faites glisser une ligne sur un dossier dans la colonne de dossiers. Faites glisser une ligne sélectionnée pour déplacer toutes les lignes sélectionnées. Déposer sur **Tous les documents** les sort de leur dossier.
- Ou cochez des lignes, ouvrez **Actions** et choisissez **Déplacer vers un dossier**.
- Sur la page d’un document ou d’un modèle, **Déplacer vers un dossier** se trouve dans le menu **Plus**.
- Un document créé avec **Nouveau document** pendant qu’un dossier est ouvert va dans ce dossier.

Le déplacement fonctionne à tout statut et est consigné dans le journal d’audit sous la forme `document.moved`.

### Qui peut faire quoi

| | Dossiers de documents | Dossiers de modèles |
| --- | --- | --- |
| Voir les dossiers | Tout le monde | Tout le monde |
| Créer un dossier | Toute personne qui peut envoyer des documents (`documents.create`) | `templates.manage` |
| Renommer, déplacer ou supprimer un dossier | La personne qui l’a créé, et toute personne ayant `documents.manage_all` | `templates.manage` |
| Déplacer un élément dans un dossier | Toute personne autorisée à modifier cet élément | Toute personne autorisée à modifier ce modèle |

Les dossiers sont partagés avec toute l’organisation, mais **un dossier n’accorde jamais d’accès**. Chacun ne voit que les documents qu’il pouvait voir auparavant : les siens, ceux partagés avec lui, ou tous si son rôle le permet. Deux personnes qui ouvrent le même dossier peuvent y voir des documents différents, et le nombre affiché à côté d’un dossier ne compte que ce que vous pouvez voir. Voir [Utilisateurs et rôles](users-and-roles.md).

### Supprimer un dossier

Supprimer un dossier ne supprime rien de ce qu’il contient. Ses documents, modèles et sous-dossiers remontent dans le dossier où il se trouvait, ou au niveau supérieur. Si le nom d’un sous-dossier y est déjà pris, le nom du dossier supprimé lui est ajouté entre parenthèses.

## Archive

Archivez les documents terminés et les modèles que vous n’utilisez plus, pour que les listes habituelles restent courtes.

- Les documents peuvent être archivés une fois **terminés**, **refusés**, **annulés** ou **expirés**. Les brouillons et les documents en attente ne le peuvent pas.
- Les modèles peuvent être archivés à tout moment, sauf si un envoi groupé qui en provient est encore en cours. Un modèle archivé n’est pas proposé dans **Nouveau document** et ne peut pas servir à un nouveau document ni à un [envoi groupé](bulk-send.md) tant qu’il n’est pas restauré.
- L’onglet **Archivé**, à côté des onglets de statut, liste l’archive, et la recherche et les dossiers y fonctionnent aussi. Les documents archivés gardent leur dossier.
- Archivez-en plusieurs à la fois en cochant des lignes et en choisissant **Actions → Archiver**, ou utilisez **Archiver** dans le menu **Plus** de la page d’un document. **Restaurer** les ramène.

Archiver demande la même autorisation que modifier le document ou le modèle. C’est consigné dans le journal d’audit sous les formes `document.archived` et `document.unarchived`.

### Archive et conservation

L’archive n’est pas une corbeille ni un stockage distinct. Les [règles de conservation](retention.md) suppriment les documents archivés selon le même calendrier que les autres, et un document sous conservation légale y reste. Les documents archivés restent téléchargeables et vérifiables.

## Depuis l’API

| Méthode | Chemin | |
| --- | --- | --- |
| GET | `/folders?scope=documents` | Liste les dossiers : `documents` ou `templates`, avec `parentId` et `itemCount` |
| POST | `/folders` | En crée un. Corps : `scope`, `name`, `parentId` facultatif |
| PUT | `/folders/{id}` | Le renomme ou le déplace. Corps : `name` et `parentId` (`null` pour le niveau supérieur) |
| DELETE | `/folders/{id}` | Le supprime ; son contenu remonte d’un niveau |
| POST | `/documents/move` | Corps : `ids` (jusqu’à 200) et `folderId` (`null` pour aucun dossier) |
| POST | `/documents/archive` | Corps : `ids` (jusqu’à 200) et `archived` (`true` pour archiver, `false` pour restaurer) |

`GET /documents` et `GET /templates` acceptent `folderId` (uniquement ce qui se trouve directement dans ce dossier) et `archived=true` (l’archive au lieu de la liste normale). `POST /documents` accepte `folderId`. `documents.counts` renvoie un nombre `archived` à côté des nombres par statut. Voir [API REST](api.md).
