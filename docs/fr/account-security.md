# Connexion, clés d’accès et deux facteurs

Votre compte est protégé par un mot de passe et, idéalement, un second facteur : une application d’authentification ou une clé d’accès. Gérez les deux sous **Paramètres → Compte**.

## Mots de passe

- Au moins 12 caractères. Les longues phrases secrètes sont les bienvenues.
- Les mots de passe sont hachés avec Argon2id. Personne, admins compris, ne peut les lire.
- Après des échecs de connexion répétés depuis une même adresse, cette adresse est verrouillée pour ce compte pendant un temps, à partir d’une minute et jusqu’à une heure. Une journée sans échec les fait oublier. Seuls des échecs provenant de plusieurs adresses à la fois peuvent verrouiller le compte pour tout le monde, et une clé d’accès permet de se connecter même alors. Chaque tentative est consignée.
- **Changer le mot de passe** demande l’actuel et déconnecte vos autres sessions.
- **Mot de passe oublié ?** sur la page de connexion envoie par e-mail un lien de réinitialisation valable une heure. Une réinitialisation déconnecte toutes les sessions et révoque vos clés d’API, et vous recevez un e-mail si certaines ont été révoquées. Si votre compte a une clé d’accès, il vous la faut quand même pour vous connecter ensuite.

## Deux facteurs avec une application d’authentification

1. Sous **Authentification à deux facteurs**, lancez la configuration et scannez le code QR avec une application d’authentification, comme 1Password, Google Authenticator ou Microsoft Authenticator.
2. Saisissez le code à 6 chiffres pour confirmer.
3. Enregistrez les **codes de récupération**. Chacun vous connecte une fois si vous perdez votre téléphone. Ils ne sont affichés que cette seule fois.

Dès lors, la connexion demande un code après votre mot de passe. Si vous perdez à la fois votre téléphone et vos codes de récupération, un admin peut réinitialiser vos deux facteurs (voir [Utilisateurs et rôles](users-and-roles.md)).

## Clés d’accès

Une clé d’accès vous permet de vous connecter avec Face ID, Touch ID, Windows Hello, votre téléphone ou une clé de sécurité, sans saisir de mot de passe.

1. Sous **Clés d’accès**, choisissez **Ajouter une clé d’accès**, donnez-lui un nom (par exemple « MacBook ») et confirmez votre mot de passe.
2. Suivez les instructions de votre navigateur.

Choisissez ensuite **Se connecter avec une clé d’accès** sur la page de connexion. Une clé d’accès vous vérifie toujours sur l’appareil (avec une empreinte, un visage ou un code PIN) ; elle compte donc à elle seule comme deux facteurs et aucun code n’est demandé. Vous pouvez en ajouter plusieurs et supprimer n’importe laquelle.

Dès que votre compte a une clé d’accès et pas d’application d’authentification, votre mot de passe seul ne suffit plus pour vous connecter : après le mot de passe, la clé d’accès vous est demandée aussi. Si vous perdez toutes vos clés d’accès, un admin peut réinitialiser vos deux facteurs, ce qui les supprime.

## Confirmer que c’est vous

Créer une clé d’API ou connecter une application donne un accès durable ; OpenSign demande donc d’abord votre mot de passe ou une clé d’accès, sauf si vous vous êtes connecté ou avez confirmé au cours des dix dernières minutes.

## Exiger les deux facteurs pour tout le monde

Un admin peut activer **Exiger l’authentification à deux facteurs pour tous** dans **Paramètres → Sécurité**. Toute personne sans application d’authentification ni clé d’accès doit alors en configurer une avant de pouvoir faire quoi que ce soit d’autre. Les clients MCP connectés avec un tel compte sont refusés jusqu’à ce qu’elle l’ait fait. Nous recommandons de l’activer.

## Authentification unique

Si votre organisation utilise l’[authentification unique](single-sign-on.md), choisissez son bouton sur la page de connexion. Votre fournisseur d’identité gère alors votre mot de passe, votre second facteur et votre e-mail, et **Paramètres → Compte** n’a ni clés d’accès, ni application d’authentification, ni mot de passe pour vous. Les clés d’API et les clients MCP ne sont pas disponibles pour ces comptes.

## Langue

Sous **Paramètres → Compte et sécurité**, choisissez la langue dans laquelle OpenSign s’affiche pour vous : anglais, chinois (simplifié), hindi, espagnol, français, arabe, bengali, portugais, russe, ourdou ou suédois. Avant la connexion, l’application suit votre navigateur, et la page de connexion a un sélecteur. Cela ne change pas ce que voient les signataires ; ils reçoivent la langue définie sur leur document ou leur partie. Certains messages du serveur restent en anglais.

## Signature enregistrée

Sous **Paramètres → Compte et sécurité → Ma signature**, dessinez ou saisissez votre signature une fois, et ajoutez vos initiales si vous le souhaitez. Quand vous signez vous-même un document, la page de signature la propose dans un onglet **Enregistrée**. Elle n’est jamais appliquée automatiquement : vous choisissez toujours de signer, et vous pouvez dessiner ou saisir une nouvelle signature à la place.

Elle n’est proposée que si le lien de signature vous appartient : quand vous [signez vous-même un document](in-person.md#signing-a-document-yourself), ou ouvrez, une fois connecté, un lien envoyé à votre propre adresse e-mail. Une personne à qui vous remettez votre appareil pour une signature en personne ne la voit jamais. Le dessin est stocké chiffré comme tout autre fichier, et **Supprimer** l’efface. L’enregistrement et la suppression sont consignés dans le journal d’audit.

Les signataires extérieurs n’ont pas de compte ; ils peuvent donc cocher **Mémoriser ma signature dans ce navigateur** à la place. Elle reste alors dans leur navigateur seulement, proposée la fois suivante à une partie portant le même nom ; rien à leur sujet n’est stocké sur le serveur entre deux documents. Voir [La page de signature](signing-page.md#signing).

## Sessions

Les sessions sont des cookies de navigateur que les scripts ne peuvent pas lire, liés à votre installation, et stockés sur le serveur uniquement sous forme de hachage à clé. Dans **Paramètres → Sécurité**, un admin définit :

| Paramètre | Par défaut | |
| --- | --- | --- |
| Durée de session (heures) | 12 | Durée maximale d’une connexion. |
| Déconnexion après inactivité (minutes) | 60 | Durée pendant laquelle une session survit sans utilisation. |

**Se déconnecter**, dans le menu utilisateur, met fin à la session dans ce navigateur.

## Applications connectées via MCP

Sous **Applications d’IA (MCP)**, vous voyez l’adresse du serveur MCP de votre installation et les clients que vous avez connectés, comme Claude. Déconnectez-les à tout moment. Voir [MCP et Claude](mcp.md).

## Clés d’API

Les clés d’API se gèrent sous **Paramètres → Clés d’API**. Voir [API REST](api.md#api-keys).
