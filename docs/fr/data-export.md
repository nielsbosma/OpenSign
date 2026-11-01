# Exporter les données saisies

Ce que les signataires saisissent dans les champs se retrouve dans le PDF scellé, et vous pouvez aussi l’obtenir sous forme de données : en tableur, en JSON, depuis l’API, ou depuis un agent IA via MCP.

## Depuis l’application

Choisissez **Exporter les Données**, puis **Excel**, **CSV** ou **JSON** :

- Sur la page **Documents**, à côté de **Nouveau document**. L’export contient les documents que la liste affiche : l’onglet de statut, le dossier, la recherche et l’onglet d’archive que vous avez choisis, pas seulement la page que vous voyez.
- Avec des lignes sélectionnées sur la page **Documents**, sous **Actions**, pour ces seuls documents.
- Sur un modèle, dans le menu **⋯**, pour chaque document créé à partir de lui.
- Sur un envoi groupé, pour chaque document qu’il a créé.
- Sur un document, dans le menu **⋯**, pour ce seul document.

Un export contient jusqu’à 5 000 documents. Affinez les filtres pour exporter davantage en plusieurs fois.

## Ce qu’il contient

Un tableur a une ligne par document, du plus ancien au plus récent :

| Colonnes | |
| --- | --- |
| Document ID, Title, Reference, Status | Le document. Reference correspond au champ **Votre référence**. |
| Created, Sent, Completed | Heures en UTC. |
| Party 1 Name, Party 1 Email, Party 1 Signed | Chaque partie dans l’ordre de signature, puis Party 2 et ainsi de suite. |
| Une colonne par champ | Nommée d’après le libellé du champ. |

Les valeurs des champs sont écrites telles qu’OpenSign les stocke, afin de pouvoir être triées et additionnées dans un tableur :

| Champ | Valeur |
| --- | --- |
| Nombre | Un nombre simple, comme `1234.5`, quel que soit le format d’affichage du champ. Dans Excel, c’est un vrai nombre. |
| Formule | Le nombre calculé par le serveur, comme pour un champ nombre. |
| Sélecteur de date et date de signature | `YYYY-MM-DD`. |
| Case à cocher | `true` ou `false`. Dans Excel, un vrai VRAI ou FAUX. |
| Liste déroulante et groupe radio | L’option choisie. Un groupe radio forme une colonne, nommée d’après le libellé du groupe. |
| Choix multiple | Les options choisies, séparées par des virgules (une liste en JSON). |
| Téléphone | Le numéro avec son indicatif de pays, comme `+46701234567`. |
| Import d’image et de fichier | Le nom du fichier. Le fichier lui-même n’est pas exporté ; téléchargez-le depuis le document. |
| Signature | Omise : la colonne Signed de la partie indique quand elle a signé. |

Un champ qui n’est pas encore rempli est vide, et les documents en attente peuvent donc aussi être exportés. Il en va de même d’un champ qu’une condition a masqué quand la partie a signé.

### Noms de colonnes

Une colonne porte le libellé du champ, ou son type s’il n’en a pas. Quand des parties ont des champs de même libellé, comme un champ Nom chacune, la partie est ajoutée : `Name (Party 1)`, `Name (Party 2)`. Un libellé utilisé deux fois par la même partie reçoit un numéro : `Note`, `Note (2)`. Donnez des libellés clairs et uniques aux champs dans le modèle, et les colonnes suivent.

Quand vous exportez des documents issus de modèles différents, chaque libellé de champ devient une colonne, et un document qui n’a pas ce champ la laisse vide.

## Sécurité

- Seuls les documents que vous pouvez voir sont exportés. Les personnes qui n’ont pas accès à tous les documents exportent les leurs et ceux qui sont partagés avec elles.
- Exporter demande l’autorisation `documents.export`, dont disposent les admins, les gestionnaires, les expéditeurs et les auditeurs. Les lecteurs peuvent voir les valeurs d’un document partagé sur la page du document, mais ne peuvent pas exporter. Voir [Utilisateurs et rôles](users-and-roles.md#permissions).
- Les numéros d’identification personnels ne sont jamais exportés, et les fichiers importés ne sont listés que par leur nom.
- Une cellule CSV qui commence par `=`, `+`, `-` ou `@` reçoit un `'` initial, afin qu’un tableur l’affiche comme du texte au lieu de l’exécuter comme une formule. Sinon, un signataire pourrait saisir `=HYPERLINK(…)` dans un champ et le faire s’exécuter sur votre ordinateur. Les nombres négatifs sont laissés tels quels. Les fichiers Excel écrivent le texte comme du texte, rien n’a donc besoin d’être modifié de ce côté.
- Les fichiers CSV sont en UTF-8 avec une marque d’ordre des octets, pour qu’Excel affiche correctement å, ü et 中.
- Chaque export est écrit dans le [journal d’audit](audit-log.md) sous la forme `documents.exported`, avec le format, les filtres et les documents qu’il contenait.

## Depuis l’API

Les valeurs d’un document, typées :

```sh
curl https://sign.example.com/api/v1/documents/$ID/values -H "Authorization: Bearer $KEY"
```

```json
{
  "id": "019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28",
  "title": "Lease, Storgatan 1",
  "status": "completed",
  "parties": [{ "id": "…", "slot": "Party 1", "role": "signer", "name": "Anna Berg", "email": "anna@example.com", "signedAt": "2026-10-29T10:12:00Z", "…": "…" }],
  "fields": [{ "key": "Monthly rent", "label": "Monthly rent", "kind": "number", "partyId": "…", "fieldIds": ["…"], "value": 12500 }],
  "values": { "Monthly rent": 12500, "Pets": "No", "Rooms": ["Kitchen", "Bath"], "Agree to terms": true }
}
```

`values` est l’accès le plus rapide : les clés des champs associées à des valeurs typées. `fields` contient les mêmes valeurs avec leurs identifiants de champ, leurs types et leurs parties. Lire les valeurs ne demande que l’accès au document, et n’est pas consigné dans l’audit, comme l’ouverture du document.

Plusieurs documents à la fois :

```sh
curl -X POST https://sign.example.com/api/v1/exports -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" -d '{"format": "xlsx", "templateId": "…", "status": "completed"}'
```

Le corps accepte `format` (`csv`, `xlsx` ou `json`) et n’importe lequel de `ids`, `templateId`, `bulkId`, `status`, `search`, `folderId` et `archived` (`true` pour les seuls documents archivés, `false` pour les exclure, omis pour les deux). La réponse contient `name`, `contentType` et `count`, plus `contentBase64` pour CSV et Excel ou `documents` (chacun comme ci-dessus) pour JSON.

Les clients DocuSign obtiennent les mêmes valeurs avec `GET /envelopes/{id}/form_data` ; voir [API DocuSign](docusign.md).

Les deux appels sont aussi des outils MCP, `documents_values` et `documents_export`, de sorte qu’un agent puisse répondre à « combien de locataires ont dit oui aux animaux ce mois-ci ? » Voir [MCP](mcp.md).

## Avec les webhooks

L’événement `document.completed` porte un `valuesUrl`, l’adresse `/values` ci-dessus, afin qu’une intégration puisse récupérer en un seul appel tout ce que les signataires ont saisi. Les webhooks eux-mêmes ne transportent jamais de valeurs de champs. Voir [Webhooks](webhooks.md#events).
