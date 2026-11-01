# Vérifier un document

Toute personne qui détient un PDF signé peut vérifier qu’il est authentique et inchangé. Aucun compte n’est nécessaire.

## Sur la page de vérification

Ouvrez `https://<your-install>/verify` (aussi accessible via **Vérifier un document** dans la barre latérale de l’application, et imprimé sur les pages de preuve de chaque document scellé) et déposez le PDF.

Le fichier est contrôlé sur le serveur et n’est pas conservé. La page répond à deux questions :

1. **Cette installation a-t-elle scellé exactement ce fichier ?** « Scellé par cet OpenSign », avec le titre du document et son heure d’achèvement, signifie que le SHA-256 du fichier correspond à un document que ce serveur a scellé et que son cachet tient, réalisé avec l’un des certificats de cachet de ce serveur. « Ce n’est pas un document scellé par ce serveur » signifie qu’il a été modifié après le scellement, ou qu’il vient d’ailleurs.
2. **Les signatures sont-elles intactes ?** Chaque signature et chaque horodatage du fichier sont listés avec leur auteur, leur date et leur état d’intégrité. Intact signifie seulement que les octets sont tels qu’ils ont été signés : n’importe qui peut créer un certificat portant n’importe quel nom. Une signature intacte mais pas faite par ce serveur le précise, et le nom qu’elle porte n’est pas confirmé.

## Ce que couvre le contrôle

Pour chaque signature et chaque horodatage de document du fichier, OpenSign vérifie :

- que la cryptographie de la signature est valide et couvre bien les octets qu’elle prétend couvrir,
- si elle couvre l’ensemble du fichier, ou seulement une révision antérieure,
- si tout ce qui a été ajouté après elle n’est que des données de validation : un magasin de sécurité du document avec des certificats et des réponses de révocation, et des horodatages de document invisibles,
- la signature propre de l’autorité d’horodatage (RSA de 2048 bits ou plus, ECDSA P-256 et P-384, SHA-2), que son certificat est destiné à l’horodatage et qu’il était valide au moment de l’horodatage. Un horodatage qui échoue à ces contrôles n’affiche pas d’heure.

Toute autre modification après une signature, comme une nouvelle annotation, une modification de formulaire, une entrée de catalogue changée, un objet supprimé ou un nouvel objet que rien ne semble utiliser, marque cette signature comme non intacte. Il en va de même d’une mise à jour qui n’est pas écrite de la façon simple dont OpenSign écrit les siennes. Cela ferme l’astuce courante consistant à modifier un PDF signé par une mise à jour incrémentielle que certains lecteurs ne signalent pas, et cela signifie qu’un fichier auquel un autre outil a ajouté des données de validation peut apparaître comme modifié.

OpenSign ne vérifie pas les signataires ni les autorités d’horodatage par rapport à une liste de confiance. Pour cela, utilisez un lecteur PDF qui en possède une.

## Dans un lecteur PDF

Les documents scellés sont des fichiers PAdES standard : Adobe Acrobat, Foxit et d’autres lecteurs qui comprennent les signatures affichent donc le cachet, l’horodatage et si le document a changé. Comme le certificat du cachet est auto-signé par défaut, un lecteur peut indiquer qu’il ne connaît pas l’identité du signataire ; la page de vérification ci-dessus fait foi pour votre installation. Voir [PDF scellés et preuves](sealed-pdfs.md#the-seal-certificate).

## Depuis l’API

`POST /api/v1/verify` avec le PDF brut comme corps (jusqu’à 40 Mo) renvoie le même résultat en JSON. Il ne demande aucune clé d’API et son débit est limité par adresse. Voir [API REST](api.md).

```sh
curl -X POST https://sign.example.com/api/v1/verify \
  -H "Content-Type: application/pdf" --data-binary @signed.pdf
```
