# Mappar och arkiv

Mappar grupperar dokument och mallar, och arkivet håller färdigt arbete ur vägen. Båda ändrar bara var saker listas. De ändrar aldrig vem som kan se ett dokument, hur det bevaras eller vad som händer med det.

## Mappar

Sidorna **Dokument** och **Mallar** har en mappkolumn till vänster. **Alla dokument** (eller **Alla mallar**) listar allt, oavsett vilken mapp det ligger i; om du väljer en mapp listas det som ligger direkt i den. På smala skärmar är mapparna en meny ovanför listan.

- Dokument och mallar har separata mappar. Ett dokument kan inte läggas i en mallmapp eller tvärtom.
- Mappar kan vara nästlade på upp till tre nivåer, till exempel *Kunder / Acme / 2026*.
- Namnen är unika inom en mapp, utan hänsyn till versaler och gemener.
- Ett dokument eller en mall ligger i högst en mapp, eller i ingen.

### Flytta saker till en mapp

- Dra en rad till en mapp i mappkolumnen. Dra en markerad rad för att flytta alla markerade rader. Om du släpper på **Alla dokument** tas de ut ur sin mapp.
- Eller markera rader, öppna **Åtgärder** och välj **Flytta till mapp**.
- På en dokument- eller mallsida finns **Flytta till mapp** i menyn **Mer**.
- Ett dokument som skapas med **Nytt dokument** medan en mapp är öppen hamnar i den mappen.

Flytt fungerar i alla statusar och registreras i granskningsspåret som `document.moved`.

### Vem som kan göra vad

| | Dokumentmappar | Mallmappar |
| --- | --- | --- |
| Se mapparna | Alla | Alla |
| Skapa en mapp | Alla som kan skicka dokument (`documents.create`) | `templates.manage` |
| Byta namn på, flytta eller radera en mapp | Den som skapade den, och alla med `documents.manage_all` | `templates.manage` |
| Flytta ett objekt till en mapp | Alla som får ändra objektet | Alla som får ändra mallen |

Mappar delas med hela organisationen, men **en mapp ger aldrig åtkomst**. Människor ser bara de dokument de kunde se tidigare: sina egna, de som delats med dem, eller alla om deras roll tillåter det. Två personer som öppnar samma mapp kan se olika dokument i den, och siffran bredvid en mapp räknar bara det du kan se. Se [Användare och roller](users-and-roles.md).

### Radera en mapp

Att radera en mapp raderar ingenting i den. Dess dokument, mallar och undermappar flyttas upp till mappen den låg i, eller till översta nivån. Om en undermapps namn redan är upptaget där läggs den raderade mappens namn till inom parentes.

## Arkiv

Arkivera färdiga dokument och mallar som du inte längre använder, så att de vanliga listorna hålls korta.

- Dokument kan arkiveras när de är **klara**, **avböjda**, **avbrutna** eller **utgångna**. Utkast och dokument som väntar kan inte arkiveras.
- Mallar kan arkiveras när som helst, om inte ett massutskick från dem fortfarande pågår. En arkiverad mall erbjuds inte i **Nytt dokument** och kan inte användas för ett nytt dokument eller ett [massutskick](bulk-send.md) förrän den återställs.
- Fliken **Arkiverat** bredvid statusflikarna listar arkivet, och sökning och mappar fungerar även där. Arkiverade dokument behåller sin mapp.
- Arkivera flera på en gång genom att markera rader och välja **Åtgärder → Arkivera**, eller använd **Arkivera** i menyn **Mer** på en dokumentsida. **Återställ** tar tillbaka dem.

Att arkivera kräver samma behörighet som att ändra dokumentet eller mallen. Det registreras i granskningsspåret som `document.archived` och `document.unarchived`.

### Arkiv och gallring

Arkivet är inte en papperskorg och inte ett separat lager. [Gallringsregler](retention.md) raderar arkiverade dokument enligt samma schema som alla andra, och ett dokument under rättslig spärr förblir spärrat. Arkiverade dokument går fortfarande att ladda ner och verifiera.

## Från API:et

| Metod | Sökväg | |
| --- | --- | --- |
| GET | `/folders?scope=documents` | Lista mappar: `documents` eller `templates`, med `parentId` och `itemCount` |
| POST | `/folders` | Skapa en. Body: `scope`, `name`, valfri `parentId` |
| PUT | `/folders/{id}` | Byt namn på den eller flytta den. Body: `name` och `parentId` (`null` för översta nivån) |
| DELETE | `/folders/{id}` | Radera den; dess innehåll flyttas upp en nivå |
| POST | `/documents/move` | Body: `ids` (upp till 200) och `folderId` (`null` för ingen mapp) |
| POST | `/documents/archive` | Body: `ids` (upp till 200) och `archived` (`true` för att arkivera, `false` för att återställa) |

`GET /documents` och `GET /templates` tar `folderId` (bara det som ligger direkt i den mappen) och `archived=true` (arkivet i stället för den vanliga listan). `POST /documents` tar `folderId`. `documents.counts` returnerar ett `archived`-antal bredvid antalen per status. Se [REST API](api.md).
