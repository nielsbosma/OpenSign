# PDF scellés et preuves

Quand la dernière partie a signé, OpenSign transforme le document en un seul PDF scellé. Il contient le document avec chaque valeur et chaque signature apposées, des pages de preuve qui racontent le déroulement de la signature, et un cachet numérique qui montre que le fichier n’a pas changé depuis. Chacun peut le vérifier avec un lecteur PDF ordinaire ou sur la [page de vérification](verification.md), et il reste vérifiable pendant des années, hors ligne.

## Ce que contient le PDF scellé

1. **Le document**, exactement tel qu’envoyé, avec les valeurs des champs et les signatures de chaque partie apposées à leur place.
2. **Des pages de preuve** à la fin :
   - votre [logo](settings.md#logo), si vous en avez importé un, le titre du document, son identifiant et votre organisation,
   - chaque partie : nom, e-mail, rôle, comment elle s’est identifiée (par exemple « BankID suédois, numéro d’identification personnel 19xxxxxx-1234 »), quand elle a signé, et son adresse IP,
   - les événements du journal d’audit : envoyé, ouvert, identifié, signé, etc., avec les heures,
   - les pièces jointes importées par les signataires et les éventuelles [photos webcam](identification.md#webcam-photo), avec leurs hachages SHA-256,
   - le hachage SHA-256 du PDF d’origine, et où vérifier le document.
3. **Des fichiers incorporés** : une pièce jointe de preuve JSON avec les mêmes faits sous forme lisible par une machine, ainsi que les [pièces jointes des signataires](attachments.md) et les photos webcam.
4. **Le cachet** : une signature PAdES sur l’ensemble du fichier au nom de votre organisation, avec un horodatage RFC 3161.
5. **Des données de validation à long terme** et un horodatage d’archivage (voir ci-dessous).

## PAdES B-LTA

Le cachet suit PAdES Baseline B-LTA, le niveau de la norme européenne pour les documents qui doivent rester vérifiables longtemps :

- **B-B** : une signature CAdES sur l’ensemble du fichier, par le cachet de l’organisation.
- **B-T** : un horodatage RFC 3161 d’une autorité d’horodatage de confiance, qui prouve quand le cachet a été apposé.
- **B-LT** : chaque certificat dont dépendent le cachet et l’horodatage, avec des réponses OCSP ou des CRL pour chacun, stockés dans le magasin de sécurité du document du PDF. Le fichier peut être validé sans contacter personne, même après l’expiration des certificats.
- **B-LTA** : un horodatage du document sur l’ensemble, données de validation comprises.

OpenSign contrôle chaque réponse OCSP ou CRL avant de la stocker : elle doit être signée par l’émetteur ou son répondeur délégué, concerner le bon certificat et être à jour. Les données de validation et l’horodatage d’archivage sont ajoutés sous forme de mises à jour incrémentielles, de sorte que les octets couverts par le cachet ne changent jamais.

Si les données de révocation ne peuvent pas être récupérées, le document est tout de même scellé et l’absence est consignée dans le journal d’audit. Il en va de même d’un certificat qu’un répondeur signale comme révoqué. Si l’autorité d’horodatage est injoignable après plusieurs tentatives, le document est scellé sans horodatage plutôt que laissé en attente.

## Le certificat du cachet

OpenSign génère la clé du cachet sur le serveur la première fois qu’il scelle un document, sous la forme d’un certificat auto-signé au nom de votre organisation, et la stocke chiffrée. **Paramètres → Signature et cachet** affiche le sujet du certificat et sa durée de validité.

**Remplacer la clé du cachet** crée un nouveau certificat auto-signé au nom actuel de l’organisation. Les nouveaux documents sont scellés avec lui. Les documents scellés auparavant restent valides et vérifiables.

Comme le certificat est auto-signé, les lecteurs PDF affichent le cachet comme valide et inchangé, mais peuvent indiquer qu’ils ne connaissent pas l’identité du signataire tant que vous ne faites pas confiance au certificat. La page de vérification contrôle que votre installation a scellé exactement ce fichier.

## Paramètres

Dans **Paramètres → Signature et cachet** :

| Paramètre | |
| --- | --- |
| Nom du cachet | Affiché comme signataire du cachet. Vide, c’est le nom de l’organisation. |
| Motif de la signature | Le motif consigné dans le cachet. Par défaut : « Signed with OpenSign ». |
| Autorité d’horodatage (RFC 3161) | L’URL de l’autorité d’horodatage. Par défaut : `http://timestamp.digicert.com`. |

## Obtenir le PDF scellé

- **Expéditeurs** : téléchargez-le depuis la page du document, qui propose aussi l’original tel qu’envoyé.
- **Parties** : toute personne qui a une adresse e-mail le reçoit par e-mail quand le document est terminé. Les fichiers jusqu’à 10 Mo sont joints ; les plus gros arrivent sous forme de lien de téléchargement. La page de signature propose aussi **Télécharger le document signé** tant que le lien est valable.
- **API** : `GET /api/v1/documents/{id}/file?version=signed`. Voir [API REST](api.md).
- **Webhooks** : `document.completed` porte le SHA-256 du fichier scellé. Voir [Webhooks](webhooks.md).

Chaque téléchargement est consigné dans le journal d’audit.

## Autorités d’horodatage de confiance

N’importe qui peut exploiter une autorité d’horodatage ; un horodatage contrôlé seul indique donc seulement que son jeton est sain. Pour que la page de vérification dise en quels horodatages vous avez confiance, listez les empreintes SHA-256 de leurs certificats sous **Autorités d’horodatage de confiance** dans Paramètres d’administration → Signature et cachet. La liste est vide par défaut.
