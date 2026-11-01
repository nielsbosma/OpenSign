# Configuration initiale et e-mail

Une nouvelle installation vous guide en deux étapes avant que quiconque puisse l’utiliser : créer le premier admin, puis connecter l’e-mail. Tant que les deux ne sont pas faites, OpenSign renvoie chaque admin vers la page de configuration.

## Étape 1 : le premier admin

Quand OpenSign démarre sans aucun utilisateur, il affiche dans son journal un **code de configuration** à usage unique :

```
No admin yet. Open https://sign.example.com and enter the setup code …
```

Ouvrez l’installation dans un navigateur et renseignez :

| Champ | |
| --- | --- |
| Code de configuration | Le code du journal. Il prouve que vous avez accès au serveur, de sorte que personne qui trouve une nouvelle installation sur Internet ne puisse la revendiquer. |
| Organisation | Le nom de votre organisation. Les signataires le voient dans les e-mails et sur la page de signature. Vous pouvez le modifier plus tard dans **Paramètres → Organisation**. |
| Votre nom, E-mail, Mot de passe | Votre compte admin. |

Les installations lancées avec le bouton **Deploy to Sliplane** n’ont pas besoin du code : l’outil de déploiement génère un `SETUP_CODE`, et **Ouvrir OpenSign** sur sa page finale mène à `/setup#code=…`, qui le saisit à votre place. Le code se trouve dans le fragment de l’URL, que le navigateur n’envoie jamais à un serveur, et la page le retire de la barre d’adresse. Si vous ouvrez l’installation autrement, le code est affiché sur la page finale de l’outil de déploiement et dans l’environnement du service dans Sliplane. Quand `SETUP_CODE` est défini, le journal n’affiche plus de code.

Dès que le premier admin existe, le code de configuration cesse définitivement de fonctionner. Toutes les autres personnes sont invitées depuis **Paramètres → Utilisateurs** (voir [Utilisateurs et rôles](users-and-roles.md)).

## Étape 2 : l’e-mail

OpenSign envoie par e-mail les invitations à signer, les relances, les avis d’arrêt d’un document, les documents scellés, les invitations d’utilisateurs et les réinitialisations de mot de passe. La configuration demande un fournisseur d’e-mail fonctionnel, Resend ou votre propre serveur SMTP, et vous envoie un message de test. La configuration ne se termine que lorsque le fournisseur l’accepte, et aucun document ne peut partir par e-mail avant cela.

### Resend

[Resend](https://resend.com) est le fournisseur par défaut.

1. Créez un compte Resend et vérifiez le domaine depuis lequel vous voulez envoyer, sous **Domains**.
2. Créez une clé d’API sous **API Keys**, avec l’autorisation d’envoyer.
3. Dans OpenSign, saisissez la clé d’API, une **Adresse de l’expéditeur** sur le domaine vérifié (par exemple `sign@example.com`) et un **Nom de l’expéditeur**.

### SMTP

Choisissez l’onglet **Serveur SMTP** pendant la configuration, ou **SMTP** comme **Fournisseur** dans **Paramètres → E-mail**, pour envoyer via votre propre serveur de messagerie ou le relais SMTP d’un autre fournisseur.

| Champ | |
| --- | --- |
| Serveur SMTP | Le nom d’hôte, par exemple `smtp.example.com`. Sans `smtp://` et sans port. |
| Port | Généralement 587 pour STARTTLS ou 465 pour TLS. |
| Sécurité | **STARTTLS** ou **TLS**. |
| Nom d’utilisateur, Mot de passe | Un nom d’utilisateur vide envoie sans connexion. Le mot de passe est stocké chiffré. |
| Adresse de l’expéditeur, Nom de l’expéditeur | L’expéditeur. |

TLS est toujours exigé et le certificat doit être valide pour le nom du serveur, de sorte que les identifiants ne circulent jamais en clair. Il n’existe pas d’option en texte brut. Les serveurs situés sur des réseaux privés sont refusés, sauf en développement.

### Modifier l’e-mail plus tard

Les paramètres d’e-mail se trouvent dans **Paramètres → E-mail**. Modifier le fournisseur, la clé, le serveur SMTP ou l’expéditeur efface l’état « vérifié », et OpenSign demande un nouvel e-mail de test avant que des documents puissent repartir. **Répondre à** est facultatif : vide, les réponses vont à la personne qui a envoyé chaque document.

## Que configurer ensuite

Toute la configuration restante se fait dans le navigateur, sous **Paramètres**. Rien de tout cela n’est nécessaire pour commencer à envoyer des documents avec des liens par e-mail.

- **Organisation** : nom, numéro d’immatriculation, pays, fuseau horaire, langue par défaut des signataires, couleur de la marque et logo. Voir [Paramètres d’administration](settings.md).
- **SMS** : pour les invitations par SMS et les codes à usage unique. Voir [Identification](identification.md#sms-codes).
- **BankID et eID** : BankID suédois, Freja eID et d’autres eID nordiques. Voir [Identification](identification.md).
- **Signature et cachet** : le nom du cachet, le motif et l’autorité d’horodatage. Voir [PDF scellés et preuves](sealed-pdfs.md).
- **Sécurité** : exiger la connexion à deux facteurs pour tout le monde. Voir [Connexion, clés d’accès et deux facteurs](account-security.md).
- **IA** : facultatif. Voir [Assistant IA et analyse](ai.md).

## Figer des paramètres avec des variables d’environnement

Tout paramètre d’administration peut être défini avec une variable d’environnement nommée `OPENSIGN_<GROUP>_<FIELD>`, par exemple `OPENSIGN_EMAIL_API_KEY` ou `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR=true`. Un champ figé est verrouillé dans le navigateur et affiche « Défini par une variable d’environnement. » C’est pratique pour l’infrastructure as code, et pour garder un secret totalement hors de la base de données. Voir [Paramètres d’administration](settings.md#pinning-settings) pour la liste complète des noms.
