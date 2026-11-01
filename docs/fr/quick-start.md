# Démarrage rapide

Cette page vous mène de zéro à un document signé sur votre propre machine. Pour une installation de production, lisez ensuite [Déploiement](deployment.md).

## 1. Lancer OpenSign en local

Il vous faut Docker et une copie du dépôt.

```sh
git clone https://github.com/nielsbosma/OpenSign.git
cd OpenSign
docker compose up -d --wait postgres s3 mailpit converter
docker compose run --rm s3-init
docker compose --profile prod up -d --build
```

Les deux premières commandes démarrent Postgres, un stockage compatible S3, Gotenberg pour les fichiers Word et Mailpit, puis créent les buckets. La dernière construit et démarre l’image de production.

- OpenSign : http://localhost:8090
- Mailpit, une boîte de réception locale pour le développement : http://localhost:8026

L’image de production envoie de vrais e-mails ; il lui faut donc un vrai compte de messagerie (étape 3). Pour essayer OpenSign sans compte, lancez-le plutôt en mode développement, où chaque e-mail arrive dans Mailpit :

```sh
pnpm install
pnpm infra:up
cp .env.example .env
pnpm dev            # web app on http://localhost:5173
```

## 2. Créer le premier admin

Une nouvelle installation affiche dans son journal un **code de configuration**, afin que personne d’autre ne puisse la revendiquer avant vous.

```sh
docker compose logs app | grep "setup code"
```

En mode développement, il se trouve dans le terminal où tourne `pnpm dev`.

Ouvrez OpenSign, saisissez le code de configuration, votre nom, votre organisation, votre e-mail et un mot de passe. Voir [Configuration initiale et e-mail](setup.md) pour le détail de chaque étape.

## 3. Connecter l’e-mail

OpenSign envoie les invitations, les codes et les documents signés par e-mail ; il demande donc un fournisseur d’e-mail avant que quiconque puisse envoyer. Saisissez une clé d’API Resend et une adresse d’expéditeur sur un domaine que vous avez vérifié dans Resend. OpenSign vous envoie un e-mail de test et ne continue qu’une fois celui-ci passé.

En mode développement, vous pouvez laisser la clé vide : l’e-mail de test et tout ce qui suit vont dans Mailpit.

## 4. Envoyer un document

1. Allez dans **Documents** et choisissez **Nouveau document**.
2. Déposez un fichier PDF ou Word, donnez-lui un titre et créez-le.
3. Sous **Parties**, ajoutez-vous comme signataire avec votre adresse e-mail.
4. Choisissez la partie dans la barre d’outils des champs et placez un champ **Signature** sur la page.
5. Choisissez **Envoyer pour signature**.

Ouvrez l’invitation, suivez le lien, dessinez ou saisissez une signature et signez. Quelques secondes plus tard, le document est **Signé** et vous pouvez télécharger le PDF scellé depuis la page du document.

## 5. Vérifier le résultat

Ouvrez **Vérifier un document** dans la barre latérale (ou allez sur `/verify`) et déposez le PDF scellé. OpenSign contrôle le cachet et les horodatages, et confirme qu’il a scellé exactement ce fichier. Un lecteur PDF qui comprend les signatures, comme Adobe Acrobat, affiche aussi le cachet.

## Étapes suivantes

- [Déploiement](deployment.md) : le faire tourner pour de bon, sur Sliplane ou partout où des conteneurs s’exécutent.
- [Identification](identification.md) : configurer les codes SMS, BankID ou Freja.
- [Utilisateurs et rôles](users-and-roles.md) : inviter vos collègues.
- [API REST](api.md) : envoyer des documents depuis vos propres systèmes.
