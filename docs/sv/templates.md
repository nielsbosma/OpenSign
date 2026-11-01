# Mallar

En mall är ett dokument som du förbereder en gång och skickar många gånger: ett anställningsavtal, ett hyresavtal, ett samtyckesformulär. Den har samma filer, parter, fält, meddelande och inställningar som ett dokument, men skickas aldrig själv. Varje gång du använder den gör OpenSign ett nytt utkast av den.

## Vem som kan använda mallar

- Den som kan skicka dokument ser alla mallar och kan starta ett dokument från en.
- Att skapa och ändra mallar kräver behörigheten **templates.manage**, som ansvariga och administratörer har. Se [Användare och roller](users-and-roles.md).

## Göra en mall

1. Gå till **Mallar** och välj **Ny mall**. OpenSign skapar en mall utan titel och öppnar den.
2. Ge den en titel, ladda upp filerna och ställ in meddelande, språk, signeringsordning och påminnelser som för ett dokument.
3. Lägg till parterna. För en mall är det oftast platshållare, till exempel "Hyresgäst" eller "Anställd", med den roll, leverans och legitimering du vill ha. Den som använder mallen fyller i de riktiga namnen och kontaktuppgifterna.
4. Placera fälten för varje part. [Texttaggar](text-tags.md) i filen kan placera dem åt dig: en tagg `role` namnger platshållarparten, och en roll som mallen inte har ännu läggs till.

En mall förblir en mall; det finns inget steg för att skicka den.

## Använda en mall

- **Ett dokument**: välj **Nytt dokument**, välj mallen och ge dokumentet en titel. OpenSign kopierar filerna, parterna (med deras bilagebegäranden), fälten, meddelandet, språket, signeringsordningen och påminnelserna till ett nytt utkast. Fyll i de riktiga namnen och kontaktuppgifterna och skicka det sedan som vanligt.
- **Många dokument**: använd [Massutskick](bulk-send.md) för att skicka mallen till varje rad i ett kalkylblad.
- **Låt människor starta det själva**: slå på en [delningslänk](share-links.md). Den som har länken eller dess QR-kod bekräftar sin e-postadress och signerar sin egen kopia.
- **Från API:et**: `POST /api/v1/documents` med `templateId`, ange sedan parterna och skicka. Scrives slutpunkt `newfromtemplate` gör detsamma. Se [REST API](api.md).

För att samla in vad människor fyllde i på varje dokument som gjorts från en mall väljer du **Exportera Data** i mallens meny **⋯**. Se [Exportera ifylld data](data-export.md).

Ett dokument som gjorts från en mall är oberoende av den: att ändra mallen senare ändrar inte dokument som redan gjorts från den. Mallens filer delas, inte kopieras, så de tar ingen extra lagringsplats.

Varje dokument visar under sin titel vilken mall och version det kom från, till exempel "Från Hyresavtal, version 3".

## Versioner

OpenSign sparar en historik för varje mall. En version innehåller allt som ett dokument kopierar: titel, meddelande, språk, signeringsordning, påminnelser, inställning för AI-granskning, filer, parter med deras bilagebegäranden och varje fält med sina alternativ, validering, format, radiogrupp, skrivskyddsflagga, villkor och formel.

- **Automatiska versioner.** När någon använder en mall som har ändrats sedan sin senaste version sparar OpenSign först en ny version. Då kan varje dokument ange exakt vilken version det gjordes från, utan att någon behöver komma ihåg att spara.
- **Spara version.** Öppna **Mer → Versionshistorik** och spara det aktuella läget med en anteckning, till exempel "Ny uppsägningstid efter 2027 års hyreslag". Att spara igen utan ändringar uppdaterar bara anteckningen.
- **Rubriken** på en mall anger vilken version den är på, eller "Ändrad sedan version 4" när det finns osparade ändringar. Klicka på den för att öppna historiken.

### Versionshistorik

**Versionshistorik** listar versionerna med den nyaste först, med vem som sparade dem, anteckningen och hur många dokument som gjorts från varje. Välj en för att se:

- vad som ändrats sedan versionen före: inställningar, filer samt parter och fält som lagts till, tagits bort eller ändrats (till exempel "Fältet Adress för Hyresgäst: position, obligatoriskt ändrat"),
- de dokument som gjorts från den och som du kan se,
- **Förhandsgranska**, som visar versionens sidor med dess fält placerade som de var.

När mallen har osparade ändringar visar **Nuvarande** högst upp vad som ändrats sedan den senaste versionen.

### Återställa en version

**Återställ** sätter tillbaka mallen som den var i den versionen: filer, parter, fält och inställningar. Ingenting går förlorat. Osparade ändringar sparas först som en version, och själva återställningen blir en ny version markerad "Återställd från version 2". Dokument som redan gjorts påverkas inte.

Personnummer i versioner lagras krypterade, som på parter, och visas maskerade.

## Ersätta en fil

När avtalstexten ändras men layouten inte gör det, använd ersätt-knappen bredvid filen under **Filer** i stället för att ta bort den och placera fälten på nytt. Ladda upp den nya PDF- eller Word-filen så gör OpenSign följande:

- placerar den på samma plats bland mallens filer,
- behåller varje fält där det var, och flyttar fält på senare filer med om sidantalet ändrades,
- flyttar fält som låg på sidor som den nya filen inte har till dess sista sida och listar dem, så att du kan rätta till dem.

Om den nya filen har [texttaggar](text-tags.md#text-tags) vinner taggarna: den gamla filens fält tas bort och taggarna placeras i stället, och eventuella parter de namnger läggs till.

På en mall sparas läget före ersättningen som en version, så att den gamla filen finns kvar i historiken och kan återställas. Ersättning fungerar även på utkast. Från API:et: `POST /api/v1/documents/{id}/files/{fileId}/replace` med filen som multipart `file`. Se [REST API](api.md#documents).

## Duplicera en mall

**Mer → Duplicera** gör en kopia som heter "Hyresavtal (kopia)" i samma mapp, med allt som mallen har. Kopian får sin egen historik. Från API:et: `POST /api/v1/documents` med `isTemplate: true` och originalet som `templateId`.

## Mappar och arkiv

Mallmappar håller en lång lista med mallar i ordning, till exempel *HR* och *Försäljning*. Arkivera en mall som du inte längre använder: den försvinner från **Nytt dokument** och massutskick tills du återställer den. Se [Mappar och arkiv](folders.md).

## Mallar och gallring

Gallringsregler raderar aldrig mallar, så deras versioner finns också kvar. Om du raderar en mall raderas dess historik och de filer som bara dess versioner använde. Se [Gallring och rättslig spärr](retention.md).
