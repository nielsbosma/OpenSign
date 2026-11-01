# Exportera ifylld data

Det som undertecknare skriver i fält hamnar i den förseglade PDF:en, och du kan också hämta ut det som data: som ett kalkylblad, som JSON, från API:et eller från en AI-agent via MCP.

## Från appen

Välj **Exportera Data** och sedan **Excel**, **CSV** eller **JSON**:

- På sidan **Dokument**, bredvid **Nytt dokument**. Exporten innehåller de dokument som listan visar: den statusflik, mapp, sökning och arkivflik du har valt, inte bara den sida du ser.
- Med rader markerade på sidan **Dokument**, under **Åtgärder**, för just de dokumenten.
- På en mall, i menyn **⋯**, för varje dokument som gjorts från den.
- På ett massutskick, för varje dokument det skapade.
- På ett dokument, i menyn **⋯**, för det enskilda dokumentet.

En export innehåller upp till 5 000 dokument. Snäva in filtren för att exportera fler i omgångar.

## Vad den innehåller

Ett kalkylblad har en rad per dokument, äldst först:

| Kolumner | |
| --- | --- |
| Document ID, Title, Reference, Status | Dokumentet. Reference är din **Er referens**. |
| Created, Sent, Completed | Tider i UTC. |
| Party 1 Name, Party 1 Email, Party 1 Signed | Varje part i signeringsordning, sedan Party 2 och så vidare. |
| En kolumn per fält | Uppkallad efter fältets etikett. |

Fältvärden skrivs så som OpenSign lagrar dem, så att de går att sortera och summera i ett kalkylblad:

| Fält | Värde |
| --- | --- |
| Nummer | Ett vanligt tal, till exempel `1234.5`, oavsett vilket visningsformat fältet har. I Excel är det ett riktigt tal. |
| Formel | Talet som servern räknade ut, som för ett nummerfält. |
| Datumväljare och signeringsdatum | `YYYY-MM-DD`. |
| Kryssruta | `true` eller `false`. I Excel ett riktigt TRUE eller FALSE. |
| Rullgardinsmeny och radiogrupp | Det valda alternativet. En radiogrupp är en kolumn, uppkallad efter gruppens etikett. |
| Flerval | De valda alternativen, åtskilda med kommatecken (en lista i JSON). |
| Telefon | Numret med landsnummer, till exempel `+46701234567`. |
| Bild och filuppladdning | Filnamnet. Själva filen exporteras inte; ladda ner den från dokumentet. |
| Underskrift | Utelämnad: partens kolumn Signed anger när parten signerade. |

Ett fält som inte är ifyllt ännu är tomt, så dokument som väntar kan också exporteras. Det gäller även ett fält som ett villkor dolde när parten signerade.

### Kolumnnamn

En kolumn är uppkallad efter fältets etikett, eller efter dess typ när det inte har någon. När parter har fält med samma etikett, till exempel ett Namn-fält var, läggs parten till: `Name (Party 1)`, `Name (Party 2)`. En etikett som används två gånger av samma part får ett nummer: `Note`, `Note (2)`. Ge fälten tydliga, unika etiketter i mallen så följer kolumnerna med.

När du exporterar dokument från olika mallar blir varje fältetikett en kolumn, och ett dokument utan det fältet lämnar den tom.

## Säkerhet

- Bara dokument som du kan se exporteras. Personer utan åtkomst till alla dokument exporterar sina egna och de som delats med dem.
- Export kräver behörigheten `documents.export`, som administratörer, ansvariga, avsändare och revisorer har. Läsare kan se värdena i ett delat dokument på dokumentsidan, men kan inte exportera. Se [Användare och roller](users-and-roles.md#permissions).
- Personnummer exporteras aldrig, och uppladdade filer listas bara med namn.
- En CSV-cell som börjar med `=`, `+`, `-` eller `@` får ett inledande `'` så att ett kalkylprogram visar den som text i stället för att köra den som en formel. Annars kunde en undertecknare skriva `=HYPERLINK(…)` i ett fält och få den körd på din dator. Negativa tal lämnas orörda. Excel-filer skriver text som text, så där behöver inget ändras.
- CSV-filer är UTF-8 med byte order mark, så att Excel visar å, ü och 中 rätt.
- Varje export skrivs till [granskningsloggen](audit-log.md) som `documents.exported`, med formatet, filtren och de dokument den innehöll.

## Från API:et

Ett dokuments värden, typade:

```sh
curl https://sign.example.com/api/v1/documents/$ID/values -H "Authorization: Bearer $KEY"
```

```json
{
  "id": "019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28",
  "title": "Lease, Storgatan 1",
  "status": "completed",
  "parties": [{ "id": "…", "slot": "Party 1", "role": "signer", "name": "Anna Berg", "email": "anna@example.com", "signedAt": "2026-10-29T10:12:00Z", "…": "…" }],
  "fields": [{ "key": "Monthly rent", "label": "Monthly rent", "kind": "number", "partyId": "…", "fieldIds": ["…"], "value": 12500 }],
  "values": { "Monthly rent": 12500, "Pets": "No", "Rooms": ["Kitchen", "Bath"], "Agree to terms": true }
}
```

`values` är den snabbaste vägen in: fältnycklar kopplade till typade värden. `fields` har samma värden med sina fält-id, typer och parter. Att läsa värden kräver bara åtkomst till dokumentet och granskas inte, som att öppna dokumentet.

Många dokument på en gång:

```sh
curl -X POST https://sign.example.com/api/v1/exports -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" -d '{"format": "xlsx", "templateId": "…", "status": "completed"}'
```

Bodyn tar `format` (`csv`, `xlsx` eller `json`) och valfritt av `ids`, `templateId`, `bulkId`, `status`, `search`, `folderId` och `archived` (`true` för bara arkiverade dokument, `false` för att utesluta dem, utelämnad för båda). Svaret har `name`, `contentType` och `count`, plus `contentBase64` för CSV och Excel eller `documents` (var och en som ovan) för JSON.

DocuSign-klienter får samma värden från `GET /envelopes/{id}/form_data`; se [DocuSign API](docusign.md).

Båda anropen är också MCP-verktyg, `documents_values` och `documents_export`, så en agent kan svara på "hur många hyresgäster svarade ja till husdjur den här månaden?" Se [MCP](mcp.md).

## Med webhooks

Händelsen `document.completed` bär en `valuesUrl`, adressen `/values` ovan, så att en integration kan hämta allt som undertecknarna angav i ett enda anrop. Själva webhooks bär aldrig fältvärden. Se [Webhooks](webhooks.md#events).
