# Bilagor från undertecknare

Du kan be en part ladda upp filer medan parten signerar: en kopia av ett leg, ett intyg, ett kontoutdrag. Filerna rensas, lagras krypterade och förseglas in i den signerade PDF:en tillsammans med resten av beviset.

## Be om bilagor

I partdialogen, under **Be om bilagor**, lägger du till en rad per fil du vill ha:

| Fält | |
| --- | --- |
| Namn | Vad du ber om, till exempel "Kopia av leg". Visas för parten. |
| Beskrivning | Valfritt. Mer detaljer till parten. |
| Obligatoriskt | En obligatorisk bilaga måste laddas upp innan parten kan signera. Valfria kan hoppas över. |

Du kan be varje part om upp till 10 bilagor. Bilagebegäranden kopieras när ett dokument görs från en [mall](templates.md), och kan anges via API:et med partens lista `attachments`.

## Vad undertecknaren ser

Signeringssidan listar de begärda filerna under **Bilagor**, var och en med **Ladda upp**, **Byt fil** och **Ta bort**. Parten kan ändra sina uppladdningar tills parten signerar.

- Godtagna typer: PDF, JPEG och PNG, upp till 10 MB var.
- Filer kontrolleras utifrån sitt innehåll, inte sitt namn.

## Så rensas filerna

- **PDF:er** skrivs om som alla andra uppladdningar: ingen JavaScript, inga formuläråtgärder, inbäddade filer eller XFA, och högst 50 sidor.
- **Bilder** avkodas och kodas om som PNG. Det tar bort alla metadata, till exempel GPS-positioner i mobilfoton, och allt som är gömt i filen. Mycket stora bilder (över 40 megapixlar) avvisas.
- Allt annat avvisas.

## I den förseglade PDF:en

När dokumentet förseglas är varje bilaga:

- listad på bevissidorna med parten, det namn du bad om, filnamnet och dess SHA-256-hash, och
- inbäddad i den förseglade PDF:en som en filbilaga, så att den följer med dokumentet och täcks av sigillet.

Öppna bilagepanelen för den förseglade PDF:en i din PDF-läsare för att se dem. Se [Förseglade PDF:er och bevis](sealed-pdfs.md).
