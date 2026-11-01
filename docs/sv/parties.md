# Parter, roller och signeringsordning

En part är var och en som deltar i ett dokument: någon som signerar, godkänner eller bara får den signerade kopian. Du lägger till parter på dokumentsidan under **Parter**.

## Partens uppgifter

| Fält | |
| --- | --- |
| Namn | Obligatoriskt. Visas på signeringssidan och i beviset. |
| Roll | Signerar, Godkänner eller Får en kopia. Se nedan. |
| E-post | Behövs för leverans med e-post och för att få den signerade kopian. |
| Mobil | Med landsnummer, till exempel `+46 70 123 45 67`. Behövs för leverans med SMS och för SMS-koder. |
| Företag | Valfritt. Fyller i fält av typen Företag. |
| Språk | Partens e-post, SMS och signeringssida. **Samma som dokumentet** som standard. Se [Signeringssidan](signing-page.md#languages). |
| Skicka länk via | Hur parten får sin signeringslänk. Se [Leverans](#delivery). |
| Legitimera för att visa | Vad parten måste göra innan hon eller han kan öppna dokumentet. Se [Legitimering](identification.md). |
| Legitimera för att signera | Vad parten måste göra innan hon eller han kan signera. Se [Legitimering](identification.md). |
| Be om identifiering med webbkamera | Parten tar ett foto av sig själv före signeringen. Se [Webbkamerafoto](identification.md#webcam-photo). |
| Personnummer | För legitimering med e-legitimation. När det är angivet kan bara den personens e-legitimation signera. Lagras krypterat och visas maskerat. |
| Signeringsordning | När dokumentet signeras i ordning går lägre nummer först. Se [Signeringsordning](#signing-order). |
| Be om bilagor | Filer som parten måste eller får ladda upp. Se [Bilagor från undertecknare](attachments.md). |

## Roller

| Roll | Vad parten gör |
| --- | --- |
| **Signerar** (undertecknare) | Läser dokumentet, fyller i sina fält och signerar med en ritad eller skriven underskrift. |
| **Godkänner** (godkännare) | Läser dokumentet och godkänner det. Ingen underskriftsbild, men godkännandet finns med i beviset på samma sätt som en underskrift. |
| **Får en kopia** (läsare) | Agerar inte. Får den förseglade PDF:en när dokumentet är klart. En läsare kan inte ha fält. |

Dokumentet är klart när varje undertecknare och godkännare har agerat.

## Leverans

| Alternativ | |
| --- | --- |
| **E-post** | Standard. OpenSign mejlar en inbjudan med den personliga länken, och påminnelser om de är på. |
| **Sms** | OpenSign skickar länken som textmeddelande. Kräver en SMS-leverantör i **Inställningar → Sms**. |
| **Länk (du lämnar över den)** | OpenSign skickar ingenting. När du skickar dokumentet visar den länken som du kan föra vidare, till exempel i din egen e-post eller chatt, eller visa i din egen app med [inbäddad signering](embedding.md). |
| **På plats (på din enhet)** | Parten signerar på din dator eller surfplatta, startat från dokumentsidan. Se [Signering på plats](in-person.md). |
| **Ingen** | OpenSign skickar ingenting och visar ingen länk. Användbart för läsare som bara behöver den signerade kopian, som går till deras e-postadress. |

Alternativ som inte är inställda i den här installationen visas men kan inte väljas förrän en administratör har ställt in dem.

### Signeringslänkar

Varje part har en personlig länk. Den gäller bara för den parten, och slutar fungera när:

- dokumentet är klart, avböjt, avbrutet eller utgår,
- länkens egen livslängd tar slut: dokumentets utgångsdatum, eller annars det antal dagar som är inställt i **Inställningar → Säkerhet** (30 som standard), räknat från när parten först aviserades, eller
- en ny länk utfärdas för parten, vilket återkallar den gamla.

Den hemliga delen av länken står efter `#`, så den når aldrig serverloggar, och signeringssidan tar bort den från adressfältet. Se [Säkerhetsmodell](security.md#signing-links).

## Signeringsordning

Som standard bjuds alla in på en gång och kan signera i vilken ordning som helst. Slå på **Signera i ordning** på kortet **Parter** för att bjuda in parterna ett steg i taget:

- Varje undertecknare och godkännare visar sitt stegnummer. När du slår på ordningen utgår den från den ordning som parterna står i.
- Använd pilarna på ett partskort för att flytta en part tidigare eller senare.
- För att låta flera parter signera tillsammans i samma steg ger du dem samma **Signeringsordning**-nummer under **Redigera**. Lägre nummer går först.
- När alla i ett steg har agerat bjuder OpenSign in nästa steg.
- En part som öppnar sin länk före sin tur ser "Andra signerar före dig" och får ett e-postmeddelande när det är hennes eller hans tur.

Läsare ingår inte i ordningen. De får den förseglade PDF:en i slutet.

## Avböja

En undertecknare eller godkännare kan avböja, med en valfri orsak. Att avböja stoppar dokumentet för alla: det blir **Avböjt**, alla länkar slutar fungera, avsändaren får besked och en `document.declined`-webhook utlöses med orsaken.

## Ändra parter

Parter kan ändras medan dokumentet är ett utkast. Om du tar bort en part tas också partens fält bort. När ett dokument har skickats är dess parter fasta; för att ändra dem avbryter du dokumentet och startar ett nytt.
