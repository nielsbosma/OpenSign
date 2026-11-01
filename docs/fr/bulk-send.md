# Envoi groupé

L’envoi groupé envoie un [modèle](templates.md) à chaque ligne d’une feuille de calcul. Chaque ligne devient son propre document, avec ses propres parties et valeurs de champs, envoyé et suivi séparément : un nouveau bail pour chaque locataire, une mise à jour de la politique pour chaque employé.

## Avant de commencer

- Préparez un modèle avec les parties, les champs, le message et l’identification voulus.
- Préparez un fichier CSV ou Excel (`.xlsx`). Enregistrez d’abord les anciens fichiers `.xls` et OpenDocument `.ods` au format `.xlsx`. La première ligne nomme les colonnes. Jusqu’à 1 000 lignes et 5 Mo.
- Vous avez besoin de l’autorisation d’envoyer des documents.

## Envoyer

1. Ouvrez le modèle et choisissez **Envoi groupé** dans son menu.
2. Choisissez le fichier. OpenSign le lit et affiche chaque colonne avec sa première valeur et ce qu’elle **Remplit**.
3. Vérifiez la correspondance. Changez la cible d’une colonne, ou choisissez **Ignorer**.
4. Définissez éventuellement les **Titres des documents**, un motif avec des noms de colonnes entre accolades, comme `Lease {Address}`. Vide, c’est le titre du modèle et le nom de la première partie qui sont utilisés.
5. Corrigez les lignes qu’OpenSign liste sous **Corrigez ces lignes avant l’envoi**.
6. Envoyez. OpenSign crée et envoie les documents en arrière-plan et ouvre la page de progression de l’envoi groupé.

## Ce qu’une colonne peut remplir

| Cible | |
| --- | --- |
| Le nom, l’e-mail, le mobile, l’entreprise ou le numéro d’identification personnel d’une partie | Un jeu pour chaque partie du modèle. |
| Un champ | Tout champ du modèle qui a un libellé, rempli comme valeur de l’expéditeur pour cette ligne. Les nombres peuvent s’écrire des deux façons (`1 234,50` ou `1,234.50`), les dates en AAAA-MM-JJ (YYYY-MM-DD) ou dans le format du champ, les champs de liste par le texte de l’option, et plusieurs choix séparés par des virgules. Une valeur qui enfreint les contrôles du champ fait échouer cette ligne. |
| Titre | Le titre du document, à la place du motif. |
| Votre référence | L’identifiant externe du document, comme un numéro de client. |
| La langue d’une partie | La langue des e-mails et de la page de signature de ce signataire, sous forme de code comme `sv` ou `pt-BR`. Voir [La page de signature](signing-page.md#languages). |

## Correspondance des colonnes

OpenSign associe les colonnes par leur nom, en anglais et en suédois. Par exemple « Email », « E-post » et « Mail » correspondent tous à l’e-mail d’une partie, et « Mobile », « Mobil » et « Telefon » au numéro de mobile. Quand un modèle a plusieurs parties, une colonne qui commence par le nom de la partie, comme « Guarantor email », va à cette partie.

Si un admin a configuré l’IA, vous pouvez laisser l’IA associer les colonnes que les règles intégrées n’ont pas pu associer. Elle reçoit les noms des colonnes. Elle reçoit aussi quelques lignes d’exemple, mais seulement si un admin autorise l’envoi du texte des documents au fournisseur d’IA. Voir [Assistant IA et analyse](ai.md).

## Contrôle des lignes

Avant tout envoi, chaque ligne est contrôlée : chaque partie a besoin d’un nom, d’une adresse e-mail valide si elle reçoit le lien par e-mail, d’un numéro de mobile valide si elle le reçoit par SMS, etc. Si une ligne devait échouer, rien n’est envoyé et OpenSign liste les problèmes par numéro de ligne de la feuille de calcul. Corrigez le fichier et choisissez-le de nouveau.

## Progression

La page de l’envoi groupé montre chaque ligne avec son destinataire, son statut et un lien vers le document créé. **Envois groupés**, en bas de la page **Modèles**, liste les exécutions récentes.

**Arrêter** ignore les lignes qui ne sont pas encore envoyées. Les documents déjà envoyés restent en circulation pour signature ; annulez-les un par un si nécessaire.

**Exporter les Données** télécharge ce que chaque destinataire a renseigné, une ligne par document, au format Excel, CSV ou JSON. Voir [Exporter les données saisies](data-export.md).

## Confidentialité

Les données des lignes sont stockées chiffrées pendant l’envoi groupé, et supprimées de chaque ligne une fois son document envoyé.

## Depuis l’API

L’envoi groupé est disponible via `POST /api/v1/bulk-sends/preview` (lit un fichier et propose une correspondance, sans rien envoyer) et `POST /api/v1/bulk-sends` (envoie). Voir [API REST](api.md#bulk-send).
