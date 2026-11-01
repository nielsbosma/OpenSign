# Identification

Pour chaque partie, vous choisissez avec quelle force elle doit prouver son identité, séparément pour ouvrir le document (**S’identifier pour consulter**) et pour le signer (**S’identifier pour signer**). Ce qu’elle utilise est consigné dans le journal d’audit et sur les pages de preuve du PDF scellé.

## Méthodes

| Méthode | Pour consulter | Pour signer | Nécessite |
| --- | --- | --- | --- |
| **Aucun** | Par défaut | | Détenir le lien suffit pour ouvrir le document. |
| **Standard** | | Par défaut | Détenir le lien. Typique pour la livraison par e-mail et par SMS, puisque seul le destinataire reçoit le lien. |
| **Code SMS** (`sms_pin`) | Oui | Oui | Un fournisseur de SMS, et un numéro de mobile sur la partie. |
| **BankID suédois** (`se_bankid`) | Oui | Oui | BankID directement ou via TIC Identity. |
| **Freja eID** (`freja`) | Oui | Oui | Un contrat Freja eID. |
| **Autre eID** (`broker`) | Oui | Oui | Un courtier eID OpenID Connect, tel qu’Idura (Criipto) ou Signicat. |

Une méthode qui n’est pas configurée sur l’installation est affichée désactivée dans la boîte de dialogue de la partie. L’envoi est refusé si une partie utilise une méthode qui n’est pas prête, afin qu’un signataire ne se retrouve jamais bloqué.

## Photo webcam

Cochez **Demander une identification par webcam** sur un signataire ou un approbateur. Avant de pouvoir signer, il prend une photo de lui-même en direct avec la caméra de son ordinateur ou de son téléphone : **Activer la caméra**, **Prendre la photo**, puis **Utiliser la photo** ou **Reprendre**. Il n’y a pas de sélecteur de fichier.

- La photo est débarrassée de ses données EXIF et de localisation, stockée chiffrée, et incorporée dans le PDF scellé avec son hachage SHA-256. Le journal d’audit consigne le moment où elle a été prise.
- Sur la page du document, la carte de la partie montre la photo et le moment où elle a été prise. Chaque consultation est consignée dans le journal d’audit.
- Seule la page de signature peut utiliser la caméra. Toutes les autres pages d’OpenSign la bloquent.

Une photo prouve qui a signé, pas que la personne était bien présente : un signataire habile pourrait envoyer une image fixe plutôt qu’une image en direct. Pour une identification forte, utilisez un eID.

## Numéros d’identification personnels

Pour BankID et Freja, renseignez le **Numéro d’identification personnel** de la partie. OpenSign exige alors que l’eID désigne exactement cette personne : une identification par quelqu’un d’autre est refusée et consignée dans le journal d’audit. Sans numéro d’identification personnel, quiconque détient le lien et possède un eID pourrait signer, et le contrôle de configuration en avertit.

Le numéro d’identification personnel est stocké chiffré et affiché masqué (seuls les quatre derniers chiffres), y compris dans le PDF scellé que reçoit chaque partie. Le numéro complet renvoyé par l’eID reste chiffré dans la base de données. Lorsque vous modifiez une partie, laissez le champ vide pour conserver le numéro enregistré.

## S’identifier pour consulter et pour signer

**S’identifier pour consulter** et **S’identifier pour signer** sont deux étapes distinctes, même avec le même eID. Seule une identification faite pour signer indique à la personne ce qu’elle signe (BankID l’affiche dans l’application) : une partie qui a les deux s’identifie donc deux fois.

## Codes SMS

La partie demande un code sur la page de signature et reçoit par SMS un code à 6 chiffres.

- Un code est valable 10 minutes et autorise 5 tentatives.
- Au plus 5 codes par heure peuvent être envoyés à une partie.
- Les codes ne sont stockés que sous forme de hachage à clé.
- Le PDF scellé affiche le numéro de téléphone avec ses trois derniers chiffres seulement.

### Configurer les SMS

Dans **Paramètres → SMS**, choisissez un fournisseur :

| Fournisseur | Nom d’utilisateur | Mot de passe |
| --- | --- | --- |
| [46elks](https://46elks.com) | Nom d’utilisateur de l’API | Mot de passe de l’API |
| [Twilio](https://www.twilio.com) | Account SID | Auth token |

L’**Expéditeur** est un nom de 11 lettres au maximum, ou un numéro qui vous appartient. Le même fournisseur sert à la livraison des liens de signature par SMS.

### Limiter le coût des SMS

Les SMS coûtent de l’argent, et les liens de partage permettent à des personnes que vous ne connaissez pas d’en demander. Ces paramètres bornent la facture :

- **Pays autorisés** : les indicatifs de pays qui peuvent recevoir des SMS, par exemple `46, 47`. Vide autorise tous les pays, ce qui est l’état initial d’une installation, afin que rien de ce qui fonctionne déjà ne s’arrête. Définissez-le avec les pays où vous envoyez réellement : certains numéros à l’étranger coûtent bien plus cher qu’un numéro local.
- **Messages par heure** (200 par défaut) et **Messages par jour** (1000) : pour tout ce que l’installation envoie. Quand l’une est atteinte, les messages suivants sont refusés jusqu’à la fin de la période, et le serveur consigne un avertissement.
- **Messages par heure pour un lien de partage** (30) : pour tous les documents créés à partir d’un même lien de partage.

0 signifie aucune limite.

## BankID suédois

Le signataire s’identifie avec l’application BankID : sur le même appareil avec **Ouvrir BankID sur cet appareil**, ou en scannant un code QR avec **BankID sur un autre appareil**. Si la partie a un numéro d’identification personnel, seul le BankID de cette personne peut répondre.

Il y a deux manières de connecter BankID. Les parties choisissent « BankID suédois » dans les deux cas.

### Avec votre propre contrat BankID

1. Dans **Paramètres → BankID et eID**, activez **BankID suédois**.
2. Pour l’essayer, activez **Utiliser l’environnement de test BankID** et laissez le certificat vide. OpenSign utilise le certificat de test public de BankID, et vous pouvez signer avec un [BankID de test](https://developers.bankid.com/test-portal).
3. Pour la production, désactivez l’environnement de test et importez le certificat de partie utilisatrice que votre banque vous a remis (un fichier `.p12`) avec son mot de passe.

OpenSign communique avec BankID en TLS mutuel et ne fait confiance qu’au certificat racine de BankID lui-même, qui est intégré. Ne le remplacez sous **BankID Server CA** que si BankID vous le demande.

### Via TIC Identity

[TIC Identity](https://id.tic.io) propose BankID sans contrat propre avec une banque.

1. Dans le portail de TIC, activez OpenID Connect pour votre locataire et enregistrez `https://<your-install>/api/eid/callback` comme URI de redirection.
2. Dans **Paramètres → BankID et eID**, activez **BankID via TIC Identity** et saisissez le nom du locataire (comme dans `https://id.tic.io/<tenant>`), l’identifiant client et le secret client.

TIC est utilisé chaque fois que BankID avec votre propre certificat est désactivé. OpenSign ne fait confiance qu’au numéro d’identification personnel suédois de la revendication `nin` de TIC, et en exige un qui soit valide.

## Freja eID

1. Dans **Paramètres → BankID et eID**, activez **Freja eID**. Utilisez **Utiliser l’environnement de test Freja** pendant les essais.
2. Importez le **Certificat client** de Freja (un fichier `.pfx`) avec son mot de passe.
3. Ajoutez le **Certificat de signature des résultats Freja**. OpenSign refuse tout résultat dont la signature ne se vérifie pas avec lui.

Le signataire confirme dans l’application Freja.

## Autres eID nordiques via un courtier

Un courtier OpenID Connect tel qu’Idura (Criipto) ou Signicat donne accès à BankID norvégien et danois, à MitID, aux identifiants bancaires finlandais et à d’autres.

1. Créez une application chez le courtier et enregistrez `https://<your-install>/api/eid/callback` comme URI de redirection.
2. Dans **Paramètres → BankID et eID**, activez **Courtier eID (OpenID Connect)** et saisissez l’**URL de l’émetteur**, l’**identifiant client** et le **secret client**.
3. Facultativement, définissez **Choix de l’eID (acr_values)** pour choisir un eID, par exemple `urn:grn:authn:no:bankid`. Vide, c’est le signataire qui choisit chez le courtier.

Le signataire est envoyé chez le courtier puis ramené. OpenSign utilise le flux de code d’autorisation avec PKCE, state et nonce, et vérifie la signature, l’émetteur, l’audience et l’expiration du jeton d’identité.

L’identification doit se terminer dans le navigateur où elle a commencé : un cookie de courte durée relie les deux, ce qui vaut aussi pour TIC Identity, et une réponse qui revient ailleurs est refusée et consignée dans le journal d’audit. Gardez les cookies activés pour l’adresse de votre OpenSign.

Quand la page de signature est intégrée dans un autre site, un cookie ne peut pas être posé depuis l’intérieur du cadre. Le signataire obtient alors une fenêtre sur l’adresse de votre OpenSign qui indique pour quel document l’identification est faite, et passe chez le courtier quand il appuie sur le bouton. Cela n’est proposé que pour les parties avec livraison par lien, sur une installation qui autorise l’intégration.

## Ce qui est consigné

Pour chaque identification, OpenSign conserve la méthode, le nom et le numéro d’identification personnel (masqué) renvoyés par l’eID, l’heure, l’adresse IP et le navigateur. Les preuves d’eID sont stockées chiffrées. Elles figurent dans le journal d’audit et sur les pages de preuve du PDF scellé. Voir [PDF scellés et preuves](sealed-pdfs.md).
