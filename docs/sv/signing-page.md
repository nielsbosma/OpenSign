# Signeringssidan

Det här är vad de du skickar dokument till ser. De behöver inget konto och ingen app, bara en webbläsare. Sidan fungerar på mobiler, surfplattor och datorer, i ljust och mörkt läge.

## Öppna länken

Inbjudan kommer med e-post eller SMS, eller direkt från dig. Den anger din organisation och personen som skickade dokumentet, och till vilket datum länken fungerar.

Om parten måste legitimera sig för att visa dokumentet gör parten det först: med en SMS-kod, BankID, Freja eller en annan e-legitimation. Se [Legitimering](identification.md). Annars öppnas dokumentet direkt.

En länk som är ofullständig, återkallad, utgången eller hör till ett dokument som inte längre väntar visar ett tydligt meddelande i stället för dokumentet.

## Läsa och fylla i

Sidan visar:

- vem dokumentet kommer från, och meddelandet till parterna om det finns ett,
- vad parten ombeds göra: signera, godkänna eller få en kopia,
- hela dokumentet, med partens egna fält markerade.

Parten fyller i sina fält: text, tal, rullgardinsmenyer, flervalslistor, radiogrupper, kryssrutor, telefonnummer, datum från en datumväljare, bilder och filer som laddas upp direkt på sidan, initialer och underskrifter. Varje fält kontrolleras medan parten skriver, med fel på partens språk eller den [feltext du skrev](documents.md#checks-and-error-texts), och parten kan inte signera förrän varje fält klarar kontrollen. Fälten för namn, e-post och företag fylls i från partens uppgifter. Om du har bett om [bilagor](attachments.md) laddar parten upp dem här. Om du har bett om ett [webbkamerafoto](identification.md#webcam-photo) tar parten det här.

I ett dokument som signeras i ordning ser en part som öppnar sin länk för tidigt "Andra signerar före dig. Vi mejlar dig när det är din tur."

## Signera

En underskrift är antingen:

- **Rita**: med ett finger, en penna eller musen, eller
- **Skriv**: parten skriver sitt namn, och det visas i en underskriftsstil, eller
- **Sparad**: en underskrift som sparats tidigare, som erbjuds först när det finns en. OpenSign-användare som signerar sina egna dokument får den från sitt [konto](account-security.md#saved-signature). Andra undertecknare kan kryssa i **Kom ihåg min signatur i den här webbläsaren** när de signerar; nästa gång en part med samma namn signerar i den webbläsaren erbjuds den här, med **Glöm** för att ta bort den. Den stannar i webbläsaren och skickas ingenstans förrän parten signerar med den. Ingenting sparas under signering på plats på någon annans enhet.

Före signeringen bekräftar parten: "Jag har läst dokumentet och godkänner att signera det elektroniskt. Det är lika bindande som en underskrift på papper." Om parten måste legitimera sig för att signera gör parten det nu. Sedan väljer parten **Signera** (eller **Godkänn** för godkännare).

Parten ser en bekräftelse. När alla har signerat får parter med en e-postadress den förseglade PDF:en med e-post, och signeringssidan erbjuder **Ladda ner signerat dokument** så länge länken fortfarande gäller.

## Avböja

**Avböj** frågar efter en valfri anledning och stoppar dokumentet för alla. Avsändaren får besked, liksom varje part som redan hade en länk.

## Språk

Undertecknare kan tilltalas på elva språk: engelska, kinesiska (förenklad), hindi, spanska, franska, arabiska, bengali, portugisiska, ryska, urdu och svenska. Arabiska och urdu visas från höger till vänster.

Varje parts språk avgör inbjudnings- och påminnelsemejl, SMS-texter och signeringssida. Det är partens **Språk** om du har angett ett, annars dokumentets **Språk för undertecknare**, annars standardvärdet från **Inställningar → Organisation**.

Signeringssidan har en språkväljare i sidhuvudet. En undertecknares val gäller bara den webbläsarfliken, så det förs inte över till nästa person på en delad enhet.

Vissa texter är fortfarande på engelska eller svenska: BankID:s egna uppmaningar, felmeddelanden från servern och bevissidorna i den förseglade PDF:en.

## Granska med din AI

Undertecknare kan be sin egen AI-assistent, till exempel ChatGPT, Claude eller Copilot, att förklara dokumentet innan de signerar. Under **Granska med din AI** väljer de **Skapa granskningslänk**, och kopierar sedan en färdig prompt eller väljer **Öppna i ChatGPT**, **Claude** eller **Copilot**, vilket öppnar assistenten med prompten ifylld.

Granskningslänken är inte signeringslänken:

- Den är skrivskyddad. Den ger assistenten dokumentets text och PDF:en som den skickades, och kan inte signera, avböja eller ändra något.
- Den visas en gång och kräver samma legitimering som för att visa dokumentet.
- Den går ut efter det antal dagar som är inställt i **Inställningar → Säkerhet** (7 som standard), eller tidigare vid dokumentets tidsfrist eller signeringslänkens utgång, och tillåter högst 100 hämtningar.
- Den slutar fungera när undertecknaren återkallar den, när du utfärdar en ny signeringslänk eller när dokumentet inte längre väntar.
- Varje användning finns i granskningsspåret.

Texten markeras för assistenten som ej betrodd, och sidor med dold text, till exempel vit eller osynlig text, pekas ut så att assistenten kan varna undertecknaren för dem.

Du kan stänga av detta per dokument under **AI-granskning av signerare** i dokumentredigeraren, vilket också återkallar länkar som redan delats. En administratör kan stänga av det för alla genom att ställa in dagarna på 0.

## Förhandsgranska som en part

Välj **Förhandsgranska** i dokumentredigeraren för att se signeringssidan som en vald part kommer att se den: din profilering, meddelandet, partens språk, fält, bilagebegäranden och legitimeringssteg. Ingenting kan signeras eller skickas från förhandsgranskningen, och den registreras inte som att parten öppnade dokumentet.

## Profilering

Sidan visar din organisations namn och logotyp och använder din profilfärg, allt från **Inställningar → Organisation**. Se [Logotyp](settings.md#logo).

Sidan kan också visas inuti din egen app. Se [Inbäddad signering](embedding.md).

## Integritet och säkerhet

- Den hemliga delen av länken står efter `#`, så den skickas inte till servern i förfrågningar eller loggar, och sidan tar bort den från adressfältet när den har laddats.
- Undertecknarens åtgärder använder aldrig en inloggad OpenSign-session, även om undertecknaren råkar ha en i samma webbläsare.
- Varje steg registreras i ett manipuleringssäkert granskningsspår, som sidan berättar för undertecknaren.
