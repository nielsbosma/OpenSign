# Balises de texte, import de formulaires et détection de champs

Vous n’avez pas à faire glisser chaque champ sur la page. OpenSign peut lire les champs dans le fichier lui-même de trois façons :

- Les **balises de texte** que vous écrivez dans un fichier Word ou PDF, comme `{{Tenant Sign;role=Tenant;type=signature}}`.
- Les **formulaires PDF** : les champs d’un PDF remplissable deviennent des champs OpenSign, avec les valeurs qui y avaient été saisies.
- **Détecter les champs** dans l’éditeur suggère des champs là où la page comporte des lignes, des tirets bas ou des cases à remplir.

Les champs créés de l’une de ces manières sont des champs ordinaires. Vous pouvez les déplacer, les redimensionner, les modifier ou les supprimer avant l’envoi.

## Balises de texte

Écrivez une balise à l’endroit où un champ doit se trouver, dans le texte du document. À l’import du fichier, OpenSign transforme chaque balise en champ à l’emplacement de la balise et retire son texte de la page, de sorte que les signataires ne la voient jamais. Le reste de la ligne reste exactement où il était.

La syntaxe est la même que celle de DocuSeal : les modèles conçus pour DocuSeal fonctionnent donc sans modification :

```
{{Field Name;role=Tenant;type=signature;required=false}}
```

La première partie est le nom du champ, affiché comme libellé. Viennent ensuite des attributs `key=value` séparés par des points-virgules. Une balise peut omettre le nom et commencer par un attribut, comme `{{type=checkbox}}`.

| Attribut | |
| --- | --- |
| `type` | Le type de champ, voir ci-dessous. `text` si omis. |
| `role` | La partie qui le remplit, par son nom, comme `Tenant` ou `Landlord`. Un rôle que le document n’a pas encore est ajouté comme nouvelle partie ; renseignez ses informations avant l’envoi. Sans rôle, le champ va à la première partie qui signe. |
| `required` | `false` rend le champ facultatif. Les champs sont obligatoires par défaut, sauf les cases à cocher. |
| `readonly` | `true` montre la valeur par défaut à la partie sans lui permettre de la modifier. |
| `default` | Une valeur dont la partie part. |
| `title` | Un libellé autre que le nom. |
| `options` | Les choix d’un champ `select` ou `multiple`, séparés par des virgules (ou par `|`). |
| `option` | Le seul choix qu’un bouton `radio` représente. Les balises radio de même nom forment un groupe. |
| `format` | Un format de date comme `DD/MM/YYYY` ou `MM/DD/YYYY`, ou un format de nombre : `usd`, `eur`, `gbp`, `sek`, ou l’aspect qu’aurait 1234, comme `1 234,00 kr`. Voir [Formats de nombre et de date](documents.md#number-and-date-formats). |
| `min`, `max` | Le plus petit et le plus grand nombre, ou la date au plus tôt et au plus tard au format AAAA-MM-JJ (YYYY-MM-DD). |
| `width`, `height` | La taille du champ en points (1/72 de pouce). Sans eux, le champ est aussi large que la balise et aussi haut que son texte, et les signatures, initiales et images reçoivent un minimum raisonnable, en grandissant vers le haut à partir de la ligne. |

| Type | Champ |
| --- | --- |
| `text` | Texte |
| `signature` | Signature |
| `initials` | Initiales |
| `date` | Sélecteur de date |
| `datenow` | Date de signature, remplie automatiquement |
| `number` | Nombre |
| `select` | Liste déroulante |
| `multiple` | Choix multiple |
| `radio` | Un bouton radio |
| `checkbox` | Case à cocher |
| `phone` | Téléphone |
| `image`, `file` | Import d’image ou de fichier |
| `name`, `email`, `company` | Remplis à partir des informations de la partie (OpenSign uniquement) |

Exemples :

```
{{Client Name;role=Client}}
{{Client Sign;type=signature;role=Client}}
{{Start Date;type=date;format=DD/MM/YYYY;role=Client}}
{{Region;type=select;options=North,South,East,West}}
{{Plan;type=radio;option=Basic}} Basic   {{Plan;type=radio;option=Plus}} Plus
{{type=checkbox;role=Client}} I accept the terms
{{Amount;type=number;format=sek;min=0}}
```

Une balise inutilisable est laissée sur la page sous forme de texte, et OpenSign vous en explique la raison à l’import : un type qu’OpenSign n’a pas (comme `payment`, `stamp` ou `verification` de DocuSeal), un attribut sans valeur, une valeur que le champ n’accepte pas, ou une balise coupée sur deux lignes. Les attributs dont OpenSign n’a pas l’usage, comme les polices et les couleurs, sont ignorés.

### Écrire des balises dans Word

Saisissez les balises comme du texte ordinaire. Gardez chaque balise sur une seule ligne : raccourcissez le nom ou réduisez la police si Word la fait passer à la ligne. La taille de police de la balise détermine la hauteur du champ ; une balise en texte de 8 points donne donc un champ bas. Les balises de texte fonctionnent dans les fichiers Word, OpenDocument et RTF sur les installations dotées d’un convertisseur de documents, et dans tout PDF dont le texte peut être lu. Les pages numérisées n’ont pas de texte, donc pas de balises.

### Désactiver les balises

Les balises sont lues à chaque import. Pour importer un fichier dont le texte contient par hasard `{{` et `}}`, désactivez-les via l’API avec `textTags=false`. Les API [Scrive](scrive.md) et [DocuSign](docusign.md) ne lisent jamais les balises.

## Formulaires PDF

Un formulaire PDF remplissable (un AcroForm) devient des champs à l’import : les zones de texte deviennent des champs texte, les cases à cocher des cases à cocher, les boutons radio un groupe radio, les listes des listes déroulantes ou à choix multiple, et les zones de signature des champs de signature. Les valeurs saisies dans le formulaire deviennent des valeurs par défaut, et les champs marqués obligatoires dans le formulaire le restent.

Un champ de formulaire dont le nom commence par le nom d’une partie va à cette partie : `Tenant.Name` et `Tenant name` vont à Tenant. Les autres vont à la première partie qui signe. Le formulaire lui-même est quand même retiré du fichier, comme pour tout import, de sorte que seuls les champs d’OpenSign subsistent. Les formulaires XFA ne sont pas lus.

## Détecter les champs

**Détecter les champs**, dans l’onglet **Champs**, examine les pages à la recherche d’endroits qui demandent un champ :

- des lignes et des suites de tirets bas sur lesquelles écrire, avec le libellé à côté ou en dessous,
- des cases vides à cocher, avec le texte voisin comme libellé.

Le libellé détermine le type : Signature, Underskrift ou Firma donne un champ de signature, Name ou Namnförtydligande un nom, Date ou Datum la date de signature, Date of birth un sélecteur de date, Email, Phone et Company leurs champs respectifs, et tout le reste du texte.

Les suggestions sont dessinées avec des bordures en pointillés et rien n’est enregistré tant que vous ne les acceptez pas. Cliquez sur l’une d’elles pour l’ajouter, ou sur **Tout accepter**. Elles vont à la partie choisie dans l’onglet **Champs** ; les signatures et les initiales vont à un signataire. Survolez une suggestion et cliquez sur la croix pour l’écarter. Les endroits qui ont déjà un champ ne sont plus suggérés.

### Avec l’IA

Quand un [fournisseur d’IA](ai.md) est configuré et que **Autoriser le texte des documents** est activé, Détecter les champs propose aussi **Avec l’IA aussi**. OpenSign envoie alors au modèle le texte de chaque page avec ses positions, ainsi que les emplacements déjà trouvés, et ajoute les suggestions du modèle, marquées d’une étincelle. Il n’envoie aucune image, au plus 30 pages et environ 40 000 caractères, et l’appel est décompté du budget d’IA. Chaque détection par IA est écrite dans le [journal d’audit](audit-log.md).

## Via l’API

`POST /api/v1/documents/{id}/files` et `documents.upload` lisent les balises de texte et les champs de formulaire, sauf si `textTags` ou `formFields` vaut `false`. Le champ `fields` de la réponse indique combien de champs ont été créés par chacun, quelles parties ont été ajoutées et quelles balises n’ont pas pu être utilisées.

`documents.detect_fields` renvoie des suggestions pour un brouillon sans rien enregistrer. Passez `ai: true` pour interroger aussi le modèle d’IA. Placez celles que vous voulez avec `documents.set_fields`. Voir [API](api.md).
