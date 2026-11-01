# Liens de partage et codes QR

Un lien de partage permet à n’importe qui de démarrer seul un document à partir d’un modèle : un formulaire d’adhésion sur votre site, une décharge à l’accueil, un formulaire de consentement imprimé en code QR sur une affiche. La personne qui ouvre le lien indique son nom et son e-mail, confirme l’e-mail avec un code, puis signe sa propre copie. Rien n’est créé pour une adresse qui n’a pas été confirmée.

## Activer un lien de partage

1. Ouvrez le modèle. Vous devez pouvoir le modifier et envoyer des documents.
2. Sous **Lien public**, activez **Partager par lien ou code QR**.
3. Dans **Rempli par le visiteur**, choisissez la partie du modèle que devient la personne qui ouvre le lien, par exemple « Client ». Elle doit signer ou approuver. Toutes les autres parties restent telles qu’elles sont dans le modèle : un cosignataire de votre côté signe donc toujours après elle.
4. Copiez le lien, ou choisissez **Code QR** et téléchargez-le en PNG (pour les écrans et les documents) ou en SVG (pour l’impression).

Le lien ressemble à `https://sign.example.com/s/Xq3…`. Il est désactivé tant que vous ne l’activez pas, et le désactiver l’arrête aussitôt. Les documents créés via le lien sont envoyés au nom de l’utilisateur qui l’a activé et apparaissent dans sa liste de documents, avec pour titre celui du modèle et le nom du visiteur.

Les liens de partage exigent que l’e-mail soit configuré, puisque chaque visiteur confirme son adresse. Voir [Paramètres](settings.md).

## Ce que voit le visiteur

1. Une page de démarrage avec votre logo, le titre et le message du modèle, dans la langue du modèle. Il peut passer à n’importe laquelle des [11 langues de signature](signing-page.md#languages).
2. Il saisit son nom et son e-mail (et son numéro de téléphone, quand la partie reçoit des codes ou des liens par SMS). Un nom ne contient que des lettres, pas de chiffres ni d’adresses, puisqu’il entre dans le titre du document et dans les e-mails qui s’y rapportent. Les SMS des documents d’un lien comptent dans les limites de SMS décrites dans [Identification](identification.md).
3. OpenSign lui envoie par e-mail un code à six chiffres valable 15 minutes. Il le saisit.
4. OpenSign crée un document à partir du modèle avec cette personne comme partie, et l’envoie. La page de signature s’ouvre aussitôt ; le même lien lui est aussi envoyé par e-mail, pour qu’elle puisse y revenir plus tard.

Si d’autres parties signent avant elle dans un document qui se signe dans l’ordre, on lui indique qu’elle recevra un e-mail quand ce sera son tour.

L’identification définie sur la partie du modèle s’applique toujours. Si la partie doit s’identifier avec BankID ou un code SMS, le visiteur le fait sur la page de signature, comme tout signataire. Voir [Identification](identification.md).

## Limites

Chaque lien a des limites que vous définissez sous **Lien public** :

| Limite | Effet |
| --- | --- |
| **Par heure** | Au plus ce nombre de documents par heure (de 1 à 1000, 10 par défaut). |
| **Au total** | Au plus ce nombre de documents en tout. Laissez vide pour ne pas en fixer. |
| **Dernier jour** | Le lien cesse de fonctionner après ce jour. |

En plus de cela, OpenSign protège lui-même chaque lien :

- Chaque adresse reçoit au plus 3 codes par heure, et chaque lien au plus trois fois sa limite horaire (au moins 20) en codes, de sorte qu’un lien ne puisse pas servir à inonder la boîte de réception de quelqu’un.
- Un code autorise 5 tentatives et ne sert qu’une fois.
- Chaque adresse IP est soumise à une limite de débit sur la page de démarrage et lors de la demande ou de la saisie de codes, comme sur la page de signature.
- Les demandes non confirmées sont supprimées au bout d’un jour.

## Changer l’adresse

**Nouvelle adresse** donne au lien une nouvelle adresse. L’ancienne adresse et tous les codes QR imprimés à partir d’elle cessent aussitôt de fonctionner. Utilisez-la quand un lien a été diffusé plus loin que prévu.

## Avec l’API

- `GET /api/v1/templates/{templateId}/share-link` : le lien, ses limites et le nombre de documents qu’il a créés.
- `PUT /api/v1/templates/{templateId}/share-link` avec `enabled`, `partyId`, `perHour`, `maxTotal` et `expiresAt`.
- `POST /api/v1/templates/{templateId}/share-link/rotate` : une nouvelle adresse.

Ce sont aussi des outils MCP (`share_links_get`, `share_links_set`, `share_links_rotate`). Voir [API REST](api.md) et [MCP](mcp.md).

Les documents créés via un lien de partage déclenchent `document.created` avec `"source": "share_link"` et le `shareLinkId` du lien. Voir [Webhooks](webhooks.md#events).

## Ce qui est consigné

Le journal d’audit du modèle consigne chaque modification du lien, chaque code envoyé (avec l’adresse e-mail) et chaque document créé. Le journal d’audit du nouveau document commence par sa création à partir du lien de partage.
