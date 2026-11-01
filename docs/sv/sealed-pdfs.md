# Förseglade PDF:er och bevis

När den sista parten har signerat gör OpenSign om dokumentet till en förseglad PDF. Den innehåller dokumentet med varje värde och underskrift instämplade, bevissidor som berättar hur signeringen gick till och ett digitalt sigill som visar att filen inte har ändrats sedan dess. Vem som helst kan kontrollera den med en vanlig PDF-läsare eller på [verifieringssidan](verification.md), och den förblir verifierbar i många år, offline.

## Vad den förseglade PDF:en innehåller

1. **Dokumentet**, exakt som det skickades, med varje parts fältvärden och underskrifter instämplade på plats.
2. **Bevissidor** i slutet:
   - din [logotyp](settings.md#logo), om du har laddat upp en, dokumentets titel, dess id och din organisation,
   - varje part: namn, e-post, roll, hur parten legitimerade sig (till exempel "Swedish BankID, personal number 19xxxxxx-1234"), när parten signerade och partens IP-adress,
   - händelserna från granskningsspåret: skickat, öppnat, legitimerat, signerat och så vidare, med tider,
   - de bilagor som undertecknare laddade upp och eventuella [webbkamerafoton](identification.md#webcam-photo), med deras SHA-256-hashar,
   - SHA-256-hashen för den ursprungliga PDF:en, och var dokumentet kan verifieras.
3. **Inbäddade filer**: en JSON-bilaga med bevis som innehåller samma fakta i maskinläsbar form, samt [bilagorna från undertecknare](attachments.md) och webbkamerafotona.
4. **Sigillet**: en PAdES-signatur över hela filen i din organisations namn, med en RFC 3161-tidsstämpel.
5. **Data för långtidsvalidering** och en arkivtidsstämpel (se nedan).

## PAdES B-LTA

Sigillet följer PAdES Baseline B-LTA, den europeiska standardnivån för dokument som måste förbli verifierbara under lång tid:

- **B-B**: en CAdES-signatur över hela filen, med organisationens sigill.
- **B-T**: en RFC 3161-tidsstämpel från en betrodd tidsstämpelutfärdare, som bevisar när sigillet gjordes.
- **B-LT**: varje certifikat som sigillet och tidsstämpeln är beroende av, med OCSP-svar eller CRL:er för vart och ett, lagrade i PDF:ens dokumentsäkerhetslager. Filen kan valideras utan att kontakta någon, även efter att certifikaten har gått ut.
- **B-LTA**: en dokumenttidsstämpel över allt, inklusive valideringsdata.

OpenSign kontrollerar varje OCSP-svar eller CRL innan det lagras: det måste vara signerat av utfärdaren eller dess delegerade svarare, gälla rätt certifikat och vara aktuellt. Valideringsdata och arkivtidsstämpeln läggs till som inkrementella uppdateringar, så de byte som sigillet täcker ändras aldrig.

Om spärrdata inte kan hämtas förseglas dokumentet ändå och luckan registreras i granskningsloggen. Detsamma gäller ett certifikat som en svarare rapporterar som spärrat. Om tidsstämpelutfärdaren inte kan nås efter flera försök förseglas dokumentet utan tidsstämpel i stället för att lämnas att vänta.

## Sigillcertifikatet

OpenSign genererar sigillnyckeln på servern första gången den förseglar ett dokument, som ett självsignerat certifikat i din organisations namn, och lagrar den krypterad. **Inställningar → Signering och sigill** visar certifikatets innehavare och hur länge det är giltigt.

**Byt sigillnyckel** skapar ett nytt självsignerat certifikat i organisationens nuvarande namn. Nya dokument förseglas med det. Dokument som förseglats tidigare förblir giltiga och verifierbara.

Eftersom certifikatet är självsignerat visar PDF-läsare sigillet som giltigt och oförändrat, men kan säga att de inte känner till undertecknarens identitet förrän du litar på certifikatet. Verifieringssidan kontrollerar att din installation förseglade exakt den filen.

## Inställningar

I **Inställningar → Signering och sigill**:

| Inställning | |
| --- | --- |
| Sigillnamn | Visas som sigillets undertecknare. Tomt använder organisationens namn. |
| Skäl för underskriften | Det skäl som registreras i sigillet. Standard: "Signed with OpenSign". |
| Tidsstämpelutfärdare (RFC 3161) | Tidsstämpelutfärdarens URL. Standard: `http://timestamp.digicert.com`. |

## Hämta den förseglade PDF:en

- **Avsändare**: ladda ner den från dokumentsidan, som också erbjuder originalet som det skickades.
- **Parter**: alla med en e-postadress får den med e-post när dokumentet är klart. Filer upp till 10 MB bifogas; större kommer som en nedladdningslänk. Signeringssidan erbjuder också **Ladda ner signerat dokument** så länge länken gäller.
- **API**: `GET /api/v1/documents/{id}/file?version=signed`. Se [REST API](api.md).
- **Webhooks**: `document.completed` bär den förseglade filens SHA-256. Se [Webhooks](webhooks.md).

Varje nedladdning registreras i granskningsspåret.

## Betrodda tidsstämpelutfärdare

Vem som helst kan driva en tidsstämpelutfärdare, så en kontrollerad tidsstämpel säger i sig bara att dess token är i sin ordning. För att verifieringssidan ska kunna säga vilka tidsstämplar du litar på listar du SHA-256-fingeravtrycken för deras certifikat under **Betrodda tidsstämpelutfärdare** i Administratörsinställningar → Signering och sigill. Listan är tom som standard.
