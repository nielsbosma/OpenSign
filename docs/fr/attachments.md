# Pièces jointes des signataires

Vous pouvez demander à une partie d’importer des fichiers pendant qu’elle signe : une copie de pièce d’identité, un certificat, un relevé bancaire. Les fichiers sont nettoyés, stockés chiffrés et scellés dans le PDF signé avec le reste des preuves.

## Demander des pièces jointes

Dans la boîte de dialogue de la partie, sous **Demander des pièces jointes**, ajoutez une ligne par fichier voulu :

| Champ | |
| --- | --- |
| Nom | Ce que vous demandez, par exemple « Copie de la pièce d’identité ». Affiché à la partie. |
| Description | Facultatif. Plus de détails pour la partie. |
| Obligatoire | Une pièce jointe obligatoire doit être importée avant que la partie puisse signer. Les facultatives peuvent être ignorées. |

Vous pouvez demander jusqu’à 10 pièces jointes à chaque partie. Les demandes de pièces jointes sont copiées lorsqu’un document est créé à partir d’un [modèle](templates.md), et peuvent être définies via l’API avec la liste `attachments` de la partie.

## Ce que voit le signataire

La page de signature liste les fichiers demandés sous **Pièces jointes**, chacun avec **Ajouter**, **Remplacer** et **Supprimer**. La partie peut modifier ses envois jusqu’à ce qu’elle signe.

- Types acceptés : PDF, JPEG et PNG, 10 Mo chacun au maximum.
- Les fichiers sont contrôlés d’après leur contenu, pas leur nom.

## Comment les fichiers sont nettoyés

- Les **PDF** sont réécrits comme tout autre fichier importé : sans JavaScript, actions de formulaire, fichiers incorporés ni XFA, et avec 50 pages au maximum.
- Les **images** sont décodées puis encodées de nouveau en PNG. Cela supprime toutes les métadonnées, comme les positions GPS des photos de téléphone, et tout ce qui était caché dans le fichier. Les très grandes images (plus de 40 mégapixels) sont refusées.
- Tout le reste est refusé.

## Dans le PDF scellé

Quand le document est scellé, chaque pièce jointe est :

- listée sur les pages de preuve avec la partie, le nom que vous avez demandé, le nom du fichier et son hachage SHA-256, et
- incorporée dans le PDF scellé comme pièce jointe, de sorte qu’elle voyage avec le document et soit couverte par le cachet.

Ouvrez le panneau des pièces jointes du PDF scellé dans votre lecteur PDF pour les voir. Voir [PDF scellés et preuves](sealed-pdfs.md).
