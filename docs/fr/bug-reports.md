# Signaler un bug

Vous avez trouvé quelque chose qui ne fonctionne pas ? Signalez-le depuis OpenSign. Votre rapport devient un ticket public sur GitHub, à votre nom, pour que les personnes qui développent OpenSign voient ce que vous avez vu.

## Comment signaler

1. Cliquez sur l’icône d’insecte, **Signaler un bug**, dans le coin supérieur de la page, à côté de l’assistant. Elle reste cliquable quand une boîte de dialogue est ouverte : vous pouvez donc aussi signaler un problème dans la boîte de dialogue elle-même.
2. Un panneau s’ouvre en bas de la page. Choisissez un outil :
   - **Commentaire** : cliquez sur un élément, dites ce qui ne va pas et appuyez sur Entrée. Il reçoit un cadre numéroté.
   - **Masquer** : faites glisser sur tout ce qui est privé pour le noircir.
   - **Pause** : utilisez la page comme d’habitude, par exemple pour ouvrir d’abord un menu ou une boîte de dialogue. Échap met aussi en pause.
3. Décrivez ce qui n’a pas marché et ce que vous attendiez. Le titre est facultatif.
4. Appuyez sur **Vérifier le rapport**. Vous voyez la capture d’écran finale et chaque ligne qui sera envoyée. Modifiez ou supprimez tout ce que vous ne voulez pas partager, ou retirez la capture.
5. Cochez la case qui confirme que le rapport peut être public, puis appuyez sur **Continuer sur GitHub**.
6. Une nouvelle fenêtre s’ouvre sur le service de rapports de bugs. Elle affiche le rapport une dernière fois. Connectez-vous avec GitHub et publiez-le. La fenêtre renvoie ensuite vers votre nouveau ticket.

Il vous faut un compte GitHub créé il y a au moins une semaine. Si votre navigateur bloque la nouvelle fenêtre, OpenSign affiche un lien pour l’ouvrir vous-même.

## Ce qui est publié

Le ticket affiche :

- votre titre et votre description, et vos commentaires avec l’élément que chacun concerne,
- la capture d’écran, si vous l’avez gardée,
- la page sous forme de modèle comme `/documents/:id`, la version d’OpenSign, votre navigateur et votre système, la taille de la fenêtre et la langue de l’application,
- les titres des boîtes de dialogue ouvertes, les erreurs récentes de la console et les requêtes échouées récentes, si vous avez gardé ces lignes.

Il n’affiche jamais l’adresse de votre OpenSign, l’adresse réelle de la page, votre nom ou votre adresse e-mail dans OpenSign, ni le contenu d’un document. Les lignes techniques perdent les adresses e-mail, les adresses web, les identifiants, les jetons, les clés et les longs numéros avant de quitter OpenSign ; chacun est remplacé par `[removed]`.

Le ticket est créé par votre compte GitHub, avec le badge de l’application **OpenDeploy Bug Reports**, qui ne peut qu’ouvrir des tickets. Le service utilise votre connexion pour cette seule requête, puis la révoque.

## Ce qui est masqué dans la capture d’écran

Avant la capture, OpenSign recouvre de gris :

- tout ce qui est saisi dans un champ,
- les pages et miniatures PDF,
- les noms et adresses e-mail des parties et des utilisateurs,
- les titres des documents,
- les entrées du journal d’audit,
- les adresses de webhook et les adresses IP autorisées des clés d’API.

Les cadres noirs que vous tracez avec **Masquer** sont aussi incrustés dans l’image. Rien ne change sur la page elle-même. Vérifiez la capture à l’étape de vérification : tout autre élément privé, c’est à vous de le masquer.

## Supprimer une capture d’écran

Les captures d’écran sont publiques et conservées jusqu’à ce que quelqu’un les supprime. Chaque ticket se termine par **Supprimer cette capture d’écran**. Ouvrez-le, connectez-vous avec GitHub : l’image est supprimée et le ticket l’indique. Vous pouvez supprimer vos propres captures ; les mainteneurs du dépôt peuvent supprimer n’importe laquelle.

GitHub garde des copies en cache des images pendant un certain temps, et n’importe qui peut l’avoir enregistrée : supprimer une capture ne peut donc pas en retirer toutes les copies. C’est pourquoi l’étape de vérification vous demande de contrôler que rien de privé n’est à l’écran.

## Ce qui quitte votre OpenSign

Rien n’est envoyé avant que vous appuyiez sur **Continuer sur GitHub**. Le serveur de votre OpenSign envoie alors le rapport au service de rapports de bugs sur `sliplane.opendeploy.cloud`, qui le garde au plus une heure, jusqu’à ce que vous le publiiez. OpenSign ne détient aucun identifiant GitHub.

Chaque personne peut envoyer 10 rapports par heure, et un OpenSign 30 par heure.

## Pour les admins

Les rapports de bugs sont activés par défaut. Dans **Paramètres → Rapports de bugs**, un admin peut les désactiver, ce qui masque **Signaler un bug** pour tout le monde, ou envoyer les rapports vers un autre dépôt, comme un fork. Voir [Paramètres d’administration](settings.md#bug-reports). Chaque rapport envoyé est consigné dans le journal d’audit.

Une installation sans accès sortant à Internet ne peut pas envoyer de rapports.
