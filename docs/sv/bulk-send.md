# Massutskick

Ett massutskick skickar en [mall](templates.md) till varje rad i ett kalkylblad. Varje rad blir ett eget dokument, med egna parter och fältvärden, som skickas och följs upp separat: ett nytt hyresavtal till varje hyresgäst, en policyuppdatering till varje anställd.

## Innan du börjar

- Förbered en mall med de parter, fält, det meddelande och den legitimering du vill ha.
- Förbered en CSV- eller Excelfil (`.xlsx`). Spara äldre `.xls`- och OpenDocument-`.ods`-filer som `.xlsx` först. Första raden anger kolumnernas namn. Upp till 1 000 rader och 5 MB.
- Du behöver behörighet att skicka dokument.

## Skicka

1. Öppna mallen och välj **Massutskick** i dess meny.
2. Välj filen. OpenSign läser den och visar varje kolumn med dess första värde och vad den **Fyller i**.
3. Kontrollera kopplingen. Ändra vilken kolumns mål som helst, eller välj **Ignorera**.
4. Ställ eventuellt in **Dokumenttitlar**, ett mönster med kolumnnamn inom klammerparenteser, till exempel `Hyresavtal {Address}`. Tomt använder mallens titel och den första partens namn.
5. Rätta eventuella rader som OpenSign listar under **Rätta de här raderna innan du skickar**.
6. Skicka. OpenSign skapar och skickar dokumenten i bakgrunden och öppnar massutskickets förloppssida.

## Vad en kolumn kan fylla i

| Mål | |
| --- | --- |
| En parts Namn, E-post, Mobil, Företag eller Personnummer | En uppsättning för varje part i mallen. |
| Ett fält | Vilket mallfält som helst med en etikett, ifyllt som avsändarens värde för den raden. Tal kan skrivas på båda sätten (`1 234,50` eller `1,234.50`), datum som YYYY-MM-DD eller i fältets format, listfält med alternativens text, och flera val åtskilda med kommatecken. Ett värde som bryter mot fältets kontroller gör att den raden misslyckas. |
| Titel | Dokumentets titel, i stället för mönstret. |
| Er referens | Dokumentets externa id, till exempel ett kundnummer. |
| En parts Språk | Språket för den undertecknarens e-post och signeringssida, som en kod såsom `sv` eller `pt-BR`. Se [Signeringssidan](signing-page.md#languages). |

## Kolumnmatchning

OpenSign matchar kolumner efter namn, på engelska och svenska. Till exempel matchar "Email", "E-post" och "Mail" alla en parts e-postadress, och "Mobile", "Mobil" och "Telefon" matchar mobilnumret. När en mall har flera parter går en kolumn som börjar med partens namn, till exempel "Guarantor email", till den parten.

Om en administratör har ställt in AI kan du låta AI matcha de kolumner som de inbyggda reglerna inte klarade. Den får kolumnnamnen. Den får också några exempelrader, men bara om en administratör tillåter att dokumenttext skickas till AI-leverantören. Se [AI-assistent och granskning](ai.md).

## Radkontroller

Innan något skickas kontrolleras varje rad: varje part behöver ett namn, en giltig e-postadress om parten får länken med e-post, ett giltigt mobilnummer om parten får den med SMS, och så vidare. Om någon rad skulle misslyckas skickas ingenting och OpenSign listar problemen efter kalkylbladets radnummer. Rätta filen och välj den igen.

## Förlopp

Massutskickssidan visar varje rad med dess mottagare, dess status och en länk till dokumentet den skapade. **Massutskick** längst ned på sidan **Mallar** listar de senaste körningarna.

**Stoppa** hoppar över de rader som ännu inte har skickats. Dokument som redan skickats ligger kvar för signering; avbryt dem ett i taget vid behov.

**Exportera Data** laddar ner vad varje mottagare fyllde i, en rad per dokument, som Excel, CSV eller JSON. Se [Exportera ifylld data](data-export.md).

## Integritet

Raddata lagras krypterat medan massutskicket pågår, och tas bort från varje rad när dess dokument har skickats.

## Från API:et

Massutskick finns som `POST /api/v1/bulk-sends/preview` (läs en fil och få en föreslagen mappning, utan att skicka något) och `POST /api/v1/bulk-sends` (skicka). Se [REST API](api.md#bulk-send).
