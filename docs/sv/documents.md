# Dokument

Ett dokument i OpenSign består av en eller flera filer att signera, de parter som deltar och de fält som varje part fyller i. Du förbereder det som ett utkast och skickar det, och därefter sköter OpenSign resten tills det är signerat, avböjt, avbrutet eller utgånget.

## Dokumentets livscykel

| Status | Vad det betyder |
| --- | --- |
| **Utkast** | Du förbereder det fortfarande. Filer, parter och fält kan ändras. |
| **Väntar** | Skickat och väntar på underskrifter. Dess filer är låsta i en enda PDF som inte längre kan ändras. |
| **Signerat** | Alla som skulle agera har signerat eller godkänt. Den förseglade PDF:en är klar. |
| **Avböjt** | En part avböjde. Signeringen stoppades för alla. |
| **Avbruten** | Avsändaren (eller en ansvarig) stoppade det. |
| **Utgånget** | Tidsfristen gick ut innan alla hade signerat. |

Ett skickat dokument kan inte redigeras. För att ändra ett som väntar avbryter du det och startar ett nytt utkast.

## Skapa ett dokument

Välj **Nytt dokument** på sidan **Dokument**. Släpp en eller flera filer, eller utgå från en [mall](templates.md), och ge dokumentet en titel.

### Filer

- **PDF**-filer godtas som de är, efter rensning (se nedan).
- **Word**-filer (`.docx`, `.doc`), **OpenDocument**-filer (`.odt`) och **RTF**-filer konverteras till PDF av Gotenberg. Det kräver en konverterare i installationen; utan en sådan godtas bara PDF:er.
- Ett dokument kan ha flera filer. När du skickar det slås de ihop i ordning till en enda PDF.
- Den största tillåtna uppladdningen ställer en administratör in i **Inställningar → Säkerhet** (25 MB som standard).

Texttaggar och ifyllbara formulärfält i filen blir fält när du laddar upp den. OpenSign berättar hur många som lades till och vilka taggar som inte gick att använda. Se [Texttaggar, formulärimport och fältidentifiering](text-tags.md).

Varje PDF kontrolleras utifrån sitt innehåll, inte sitt namn, och skrivs om utan JavaScript, formuläråtgärder, inbäddade filer, XFA samt start- och URI-åtgärder. Om något togs bort berättar OpenSign vad. Ingen, inte ens undertecknarna, ser någonsin det ursprungliga aktiva innehållet.

### Detaljer

| Inställning | |
| --- | --- |
| Titel | Visas i dokumentlistan, i inbjudan och på signeringssidan. |
| Meddelande till parterna | Visas i inbjudan och på signeringssidan. |
| Språk för undertecknare | Språket för e-post, SMS och signeringssidan, om inte en part har ett eget. Tomt använder standardvärdet från **Inställningar → Organisation**. Se [Signeringssidan](signing-page.md#languages). |
| Går ut | Datumet efter vilket dokumentet inte längre kan signeras. Det blir då **Utgånget**. |
| Påminn var (dag) | Skickar en påminnelse till parter som ännu inte har signerat, så här många dagar (högst 60). 0 stänger av påminnelserna. |
| Er referens | Ett externt id, till exempel ett CRM- eller ärendenummer. Du kan söka efter det i dokumentlistan, och det ingår i webhooks och API:et. |

### Parter

Lägg till alla som signerar, godkänner eller får en kopia. För varje part väljer du roll, hur länken når personen och hur personen måste legitimera sig. **Signera i ordning** på samma kort bjuder in dem ett steg i taget. Se [Parter, roller och signeringsordning](parties.md) och [Legitimering](identification.md).

### Fält

Välj en part på fliken **Fält** och placera sedan fält på sidorna. Fält kan också komma från själva filen: [texttaggar](text-tags.md#text-tags) som `{{Sign;type=signature;role=Tenant}}` och fälten i ett [ifyllbart PDF-formulär](text-tags.md#pdf-forms) blir fält när du laddar upp, och [Hitta fält](text-tags.md#detect-fields) på fliken **Fält** föreslår fält där sidan har linjer och rutor att fylla i. Varje fält tillhör en part, som fyller i det vid signeringen. Markera ett fält för att ändra det i panelen **Fält**: vem som fyller i det, dess etikett, om det är obligatoriskt och inställningarna nedan. Skift-klicka på fält eller dra över sidan för att välja flera, och justera eller fördela dem sedan i sidopanelen. Fält fäster mot varandra när du drar; håll ned Alt för att stänga av det.

| Fält | |
| --- | --- |
| Underskrift | En ritad eller skriven underskrift. |
| Initialer | Partens initialer. |
| Text | Fri text. Kan ha ett mönster, en längd och ett förvalt värde. |
| Nummer | Ett tal, med ett minsta och ett största värde och ett [format](#number-and-date-formats). |
| Rullgardinsmeny | Ett val ur en lista med alternativ. |
| Flerval | Valfritt antal val ur en lista, skrivna kommaseparerade på sidan. |
| Radioknapp | En grupp knappar av vilka parten väljer en. Se [Radiogrupper](#radio-groups). |
| Kryssruta | Kan vara obligatorisk, eller ikryssad från början. |
| Telefon | Ett telefonnummer med landsnummer, lagrat som +46701234567. |
| Datumväljare | Ett datum som parten väljer i en kalender, med ett tidigaste och ett senaste datum och ett [format](#number-and-date-formats). |
| Bild | Parten laddar upp en bild eller tar ett foto. Det ritas in i rutan och skalas så att det ryms. |
| Fil | Parten laddar upp en PDF, JPEG eller PNG direkt på sidan. Filens namn hamnar i rutan och filen förseglas in i den signerade PDF:en, som [bilagor](attachments.md). |
| Formel | Ett tal som räknas ut från andra fält, till exempel en summa med moms. Se [Formler](#formulas). |
| Datum | Fylls i med signeringsdatumet, i det format du väljer. |
| Namn, E-post, Företag | Fylls i från partens uppgifter. |

Välj **Jag, nu (förifyllt)** på fliken **Fält** för att placera ett värde som du fyller i själv före utskick, till exempel ett avtalsnummer, ett belopp eller ett datum. Obligatoriska fält av det här slaget måste fyllas i innan dokumentet kan skickas.

En parts text-, nummer-, list-, radio-, kryssrute-, telefon- och datumfält kan ha ett **Förvalt värde** som de utgår från. Slå på **Skrivskyddat** så ser parten värdet men kan inte ändra det.

#### Kontroller och feltexter

Kontroller körs på signeringssidan medan parten skriver, och igen på servern när parten signerar, så ett värde som bryter mot en kontroll kan aldrig hamna i den förseglade PDF:en.

| Kontroll | Gäller | |
| --- | --- | --- |
| Tillåtet format | Text, initialer, företag | En förinställning (e-postadress, endast siffror, svenskt postnummer, personnummer eller organisationsnummer) eller ett eget reguljärt uttryck. Hela värdet måste stämma. |
| Minsta längd, Största längd | Text, initialer, företag | I tecken. |
| Minsta, Största | Nummer | Inklusive. |
| Tidigast, Senast | Datumväljare | Inklusive. |
| Obligatoriskt | Alla fält som parten fyller i | |

**Feltext** ersätter det inbyggda felet med din egen text, för vilken kontroll som helst på det fältet, till exempel *Skriv ordernumret som AB1234*. Utan den ser parten ett inbyggt fel på sitt eget språk.

Mönster använder den vanliga syntaxen för reguljära uttryck: klasser som `[0-9]` och `\d`, upprepning som `{3}`, `+` och `?`, alternativ med `|` och grupper. Look-ahead och bakåtreferenser stöds inte, vilket håller varje kontroll snabb.

#### Tal- och datumformat

Formatet avgör hur ett värde skrivs på sidan; det som lagras förblir enkelt (`1234.5`, `2026-12-31`), så att exporter och API:et alltid får samma form.

Skriv ett talformat så som 1234 skulle se ut i det. `1,234.00` ger ett kommatecken mellan tusental och två decimaler, `1 234,00 kr` ett mellanslag, ett decimalkomma och en valuta efter, och `$1234` inga decimaler med ett dollartecken. Värden avrundas uppåt vid halva till formatets antal decimaler. Parter kan skriva tal på båda sätten, till exempel `1 234,5` eller `1,234.50`.

Datum kan skrivas som YYYY-MM-DD, DD/MM/YYYY, MM/DD/YYYY, DD.MM.YYYY eller D MMM YYYY (31 dec 2026). Samma format gäller för signeringsdatumet.

#### Radiogrupper

Om du placerar **Radioknapp** läggs en knapp till. Markera den och välj **Lägg till en knapp i gruppen** för varje ytterligare alternativ, och dra sedan knapparna dit de hör hemma. Varje knapp står för ett alternativ; gruppen delar etikett och om ett svar krävs. Parten kan välja en knapp i gruppen.

#### Villkorliga fält

Under **Visa bara när** i panelen Fält lägger du till villkor för andra fält. Fältet visas bara så länge alla villkor gäller, till exempel *Organisationsnummer* bara när *Typ* är *Företag*, eller en *Orsak*-ruta bara när kryssrutan *Annat* är ikryssad. Fält med villkor ritas med streckad kant i redigeraren.

| Regel | |
| --- | --- |
| Är, Är inte | Jämför med ett alternativ, ett tal eller en text. Text jämförs utan hänsyn till versaler och gemener, och tal efter värde, så 10 och 10.0 är samma sak. |
| Är ifyllt, Är tomt | För en kryssruta: Är ikryssad, Är inte ikryssad. |
| Innehåller | En flervalslista har alternativet ikryssat, eller en text innehåller orden. |
| Är mer än, Är mindre än | Tal och datum (Är efter, Är före). |

Ett dolt fält är aldrig obligatoriskt, lämnas tomt när parten signerar och skrivs inte på den signerade PDF:en. Ett fält som beror på ett dolt fält ser det som tomt, så döljandet fortplantar sig.

Villkor hänvisar till fält med deras etikett, så fältet du tittar på behöver en etikett, och bara ett fält får ha den. Om du byter namn på ett fält i redigeraren uppdateras de villkor och formler som använder det. En parts fält kan titta på partens egna fält, på värden som du har förifyllt och på fält som tillhör parter som signerar tidigare när **Signera i ordning** är på. Servern kontrollerar villkoren igen när parten signerar, så en part kan inte hoppa över ett fält genom att ändra sidan.

#### Formler

Ett **Formel**-fält räknar ut ett tal från andra fält medan parten fyller i dokumentet. Parten ser resultatet men kan inte ändra det, och servern räknar ut det på nytt från de inskickade värdena när parten signerar, så det lagrade resultatet går att lita på. Ge det ett [talformat](#number-and-date-formats) för att avrunda och skriva det, till exempel `1 234,00 kr`.

Skriv andra fälts etiketter inom klammerparenteser och använd `+ - * /` (eller `× ÷`), parenteser och de här funktionerna:

| Funktion | |
| --- | --- |
| `round(x, 2)` | Avrundar uppåt vid halva till 0 till 6 decimaler; utan ett andra värde, till ett heltal. |
| `floor(x)`, `ceil(x)`, `abs(x)` | Neråt, uppåt, eller utan tecken. |
| `min(a, b, …)`, `max(a, b, …)`, `sum(a, b, …)` | |
| `if(condition, then, else)` | Villkor jämför med `=`, `!=`, `<`, `<=`, `>` och `>=`, och kombineras med `and(…)`, `or(…)` och `not(…)`. |
| `contains(list, "text")` | Om en flervalslista har alternativet ikryssat, eller en text innehåller orden. |

Exempel:

- Moms på ett pris: `{Price} * 0.25`
- En summa med moms: `round({Hours} * {Rate} * 1.25, 2)`
- En rabatt över ett belopp: `if({Amount} >= 10000, {Amount} * 0.05, 0)`
- Ett pris efter plan: `if({Plan} = "Pro", 99, 49) * {Seats}`

Ett tomt fält räknas som 0, en ikryssad kryssruta som 1. En formel som delar med noll, eller använder text som inte är ett tal, förblir tom. En formel som du fyller i själv kan bara använda dina egna förifyllda fält och räknas ut när du sparar; en parts formel kan använda samma fält som ett [villkor](#conditional-fields).

Fält lagras som andelar av sidan, så de stannar på plats oavsett sidstorlek.

## Innan du skickar

Panelen **Granska** kontrollerar dokumentets upplägg medan du arbetar och listar allt som skulle hindra det från att signeras eller göra resultatet svagare:

- Fel, som blockerar utskick: ingen fil, ingen undertecknare eller godkännare, en part som får länken med e-post eller SMS men saknar e-postadress eller telefonnummer, en part som måste bekräfta med en SMS-kod men saknar telefonnummer, en läsare med fält att fylla i, ett obligatoriskt avsändarfält som är tomt.
- Varningar, värda att titta på: en undertecknare utan underskriftsfält (underskriften visas då bara på bevissidan), en part med e-legitimation men utan personnummer (så att vem som helst med länken och en e-legitimation kunde signera), en part utan både e-post och telefon (så att den inte får den signerade kopian).

Om en administratör har ställt in AI kan samma panel också be en AI läsa dokumentet. Se [AI-assistent och granskning](ai.md#document-review).

## Förhandsgranskning

**Förhandsgranska**, bredvid **Skicka för signering**, visar signeringssidan som en vald part kommer att se den, med partens fält, språk och legitimeringssteg. Se [Signeringssidan](signing-page.md#previewing-as-a-party).

## AI-granskning av signerare

Som standard kan undertecknare ge sin egen AI-assistent en skrivskyddad kopia av dokumentet för att förklara det innan de signerar. Stäng av **Låt signerare dela med sin AI** under **Avancerat** > **AI-granskning av signerare** för att förhindra det. Se [Signeringssidan](signing-page.md#review-with-your-ai).

## Skicka

Välj **Skicka för signering**. OpenSign slår ihop filerna till en PDF, låser den och meddelar de första parterna. För parter vars leverans är **Länk** visar OpenSign **Lämna över signeringslänkarna** med varje länk att kopiera. Du kan hämta en ny länk senare från partens kort, och **Ny länk** återkallar den gamla först.

## Medan det väntar

Från dokumentsidan kan du:

- Välja **Påminn** för att påminna alla vars tur det är och som ännu inte har signerat.
- Hämta eller ersätta en parts signeringslänk.
- Starta **Signera på plats** för en part som signerar på din enhet. Se [Signering på plats](in-person.md).
- Välja **Avbryt** för att avbryta dokumentet, med en valfri orsak. Alla signeringslänkar slutar fungera och parter som har aviserats får besked.
- Se **Aktivitet**: varje steg i dokumentets granskningsspår, till exempel vem som öppnade, legitimerade sig, signerade eller laddade ner det.

## När det är klart

När den sista parten har signerat förseglar OpenSign dokumentet i bakgrunden, oftast inom några sekunder. Dokumentsidan erbjuder sedan den förseglade PDF:en och originalet som det skickades. Parter med en e-postadress får också den förseglade PDF:en. Se [Förseglade PDF:er och bevis](sealed-pdfs.md).

Varje nedladdning registreras i granskningsspåret.

För att få det parterna fyllde i som data väljer du **Exportera Data** i menyn **⋯**. Se [Exportera ifylld data](data-export.md).

## Dela med kollegor

Dokument tillhör den som skapade dem. För att låta en kollega se ett dokument delar du det med personen, och låter eventuellt personen ändra det. Ansvariga, revisorer och administratörer kan se alla dokument utan delning. Se [Användare och roller](users-and-roles.md).

## Radera

Ett dokument som inte väntar kan raderas tillsammans med sina filer. Avbryt först ett dokument som väntar. Signerade dokument kan bara raderas av någon som hanterar alla dokument, och ett dokument under rättslig spärr kan inte raderas av någon. Raderingen registreras i granskningsspåret. Dokument kan också raderas automatiskt av gallringsregler. Se [Gallring och rättslig spärr](retention.md).

## Hitta dokument

Sidan **Dokument** listar de dokument du kan se, de nyaste först, med flikar per status. Sök efter titel eller efter din referens.

Sortera dokument i mappar från mappkolumnen vid sidan av listan, och arkivera färdiga så att de lämnar statusflikarna. Fliken **Arkiverat** listar dem. Se [Mappar och arkiv](folders.md).
