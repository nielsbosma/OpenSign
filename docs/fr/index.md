# Documentation d’OpenSign

OpenSign est un service de signature électronique open source que vous hébergez vous-même. Vous importez un document Word ou PDF, placez des champs et l’envoyez. Chaque signataire reçoit un lien, prouve son identité de la manière que vous avez choisie, puis signe. Quand tout le monde a terminé, vous récupérez un PDF scellé que n’importe quel lecteur PDF peut vérifier, avec des pages de preuve qui consignent qui a signé, quand et comment chacun s’est identifié.

Cette documentation suit votre langue, et le menu de langue en haut de la page permet de la changer. Les traductions sont automatiques et la version anglaise fait foi.

Ces pages couvrent toutes les facettes d’OpenSign, pour les personnes qui envoient des documents, pour les admins qui gèrent une installation et pour les développeurs qui l’intègrent.

## Par où commencer

| Vous voulez | Lire |
| --- | --- |
| Essayer OpenSign ou le mettre en production | [Démarrage rapide](quick-start.md), puis [Déploiement](deployment.md) |
| Terminer la configuration d’une nouvelle installation | [Configuration initiale et e-mail](setup.md) |
| Envoyer votre premier document | [Documents](documents.md) et [Parties, rôles et ordre de signature](parties.md) |
| Demander aux signataires d’utiliser BankID, Freja ou des codes SMS | [Identification](identification.md) |
| Envoyer un même modèle à des centaines de personnes | [Envoi groupé](bulk-send.md) |
| Vérifier qu’un PDF signé est authentique | [Vérifier un document](verification.md) |
| Connecter un CRM ou un autre système | [API REST](api.md) et [Webhooks](webhooks.md) |
| Enregistrer les PDF signés dans Google Drive, publier dans Slack, mettre à jour HubSpot | [Automatisations](automations.md) via Composio |
| Quitter Scrive ou DocuSign | [Compatibilité Scrive](scrive.md) ou [Compatibilité DocuSign](docusign.md) |
| Utiliser OpenSign avec Claude | [MCP et Claude](mcp.md) |
| Comprendre comment OpenSign protège les documents | [Modèle de sécurité](security.md) |

## Ce que fait OpenSign

- **Parcours de signature.** Signataires, approbateurs et lecteurs, dans un ordre défini ou tous en même temps. Les liens partent par e-mail ou par SMS, vous les remettez vous-même, ou le signataire signe en personne sur votre appareil. Les relances, les dates d’expiration et le refus avec motif sont intégrés. Voir [Parties, rôles et ordre de signature](parties.md).
- **Identification.** Un lien par e-mail, un code SMS, BankID suédois (avec votre propre contrat bancaire ou via TIC Identity), Freja eID, ou d’autres eID nordiques via un courtier OpenID Connect. Avec un numéro d’identification personnel sur la partie, seule cette personne peut signer. Voir [Identification](identification.md).
- **Modèles et envoi groupé.** Enregistrez un document préparé comme modèle, puis envoyez-le à chaque ligne d’un fichier CSV ou Excel. Voir [Modèles](templates.md) et [Envoi groupé](bulk-send.md).
- **Pièces jointes des signataires.** Demandez des fichiers à une partie, comme une copie de pièce d’identité ; ils sont nettoyés puis scellés dans le PDF signé. Voir [Pièces jointes des signataires](attachments.md).
- **PDF scellés.** Des cachets PAdES B-LTA avec un horodatage RFC 3161 et tout ce qu’il faut pour valider le fichier hors ligne pendant des années. Voir [PDF scellés et preuves](sealed-pdfs.md).
- **Dossiers et archive.** Classez les documents et les modèles dans des dossiers partagés, et archivez le travail terminé pour que les listes restent courtes. Voir [Dossiers et archive](folders.md).
- **Conservation et conservation légale.** Supprimez automatiquement les documents après des durées définies, et gardez ceux qui comptent grâce à la conservation légale. Voir [Conservation et conservation légale](retention.md).
- **Utilisateurs et rôles.** Admin, gestionnaire, expéditeur, auditeur et lecteur, avec des clés d’accès ou une application d’authentification pour la connexion à deux facteurs. Voir [Utilisateurs et rôles](users-and-roles.md).
- **API.** Une API REST avec OpenAPI, des points d’accès Scrive et DocuSign interchangeables, des webhooks signés et un serveur MCP pour Claude et d’autres clients IA. Voir [API REST](api.md).
- **L’IA, si vous le souhaitez.** Un assistant et une analyse des documents avant leur envoi. Désactivés tant qu’un admin n’a pas choisi de fournisseur. Voir [Assistant IA et analyse](ai.md).
- **La sécurité d’abord.** Chiffrement au repos, journal d’audit infalsifiable, en-têtes de sécurité web stricts et contrôles d’autorisation côté serveur à chaque appel. Voir [Modèle de sécurité](security.md).

## Comment tout s’articule

OpenSign est une seule image de conteneur. Il lui faut Postgres, un bucket compatible S3 et, si vous voulez accepter les fichiers Word, un convertisseur Gotenberg sur un réseau privé. L’application web, les API, le serveur MCP et le worker d’arrière-plan s’exécutent dans la même image et utilisent les mêmes services : une autorisation ou une règle s’applique donc de la même façon, quelle que soit la manière dont la requête arrive.

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Email, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

OpenSign est distribué sous licence MIT. Le code source est sur [GitHub](https://github.com/nielsbosma/OpenSign).
