# Legitimering

För varje part väljer du hur starkt parten måste styrka vem hon eller han är, separat för att öppna dokumentet (**Legitimera för att visa**) och för att signera det (**Legitimera för att signera**). Det parten använder registreras i granskningsspåret och på bevissidorna i den förseglade PDF:en.

## Metoder

| Metod | För att visa | För att signera | Kräver |
| --- | --- | --- | --- |
| **Ingen** | Standard | | Det räcker att ha länken för att öppna dokumentet. |
| **Standard** | | Standard | Att ha länken. Typiskt för leverans med e-post och SMS, eftersom bara mottagaren får länken. |
| **Sms-kod** (`sms_pin`) | Ja | Ja | En SMS-leverantör och ett mobilnummer på parten. |
| **Svenskt BankID** (`se_bankid`) | Ja | Ja | BankID direkt eller via TIC Identity. |
| **Freja eID** (`freja`) | Ja | Ja | Ett avtal med Freja eID. |
| **Annan e-legitimation** (`broker`) | Ja | Ja | En OpenID Connect-förmedlare av e-legitimationer, till exempel Idura (Criipto) eller Signicat. |

En metod som inte är inställd i installationen visas som inaktiverad i partdialogen. Utskick avvisas om en part använder en metod som inte är redo, så en undertecknare fastnar aldrig.

## Webbkamerafoto

Kryssa i **Be om identifiering med webbkamera** på en undertecknare eller godkännare. Innan parten kan signera tar parten ett levande foto av sig själv med datorns kamera eller med mobilen: **Starta kameran**, **Ta foto**, sedan **Använd fotot** eller **Ta om**. Det finns ingen filväljare.

- Fotot rensas från EXIF- och platsdata, lagras krypterat och bäddas in i den förseglade PDF:en med sin SHA-256-hash. Granskningsspåret registrerar när det togs.
- På dokumentsidan visar partens kort fotot och när det togs. Varje visning registreras i granskningsspåret.
- Bara signeringssidan får använda kameran. Alla andra sidor i OpenSign blockerar den.

Ett foto är ett bevis på vem som signerade, inte ett bevis på att personen är närvarande: en skicklig undertecknare kunde skicka en stillbild i stället för ett levande foto. För stark legitimering, använd en e-legitimation.

## Personnummer

För BankID och Freja anger du partens **Personnummer** på parten. OpenSign kräver då att e-legitimationen anger exakt den personen: en legitimering av någon annan avvisas och registreras i granskningsspåret. Utan personnummer kan vem som helst som har länken och en e-legitimation signera, och kontrollen vid inställningen varnar för det.

Personnumret lagras krypterat och visas maskerat (bara de fyra sista siffrorna), även i den förseglade PDF:en som varje part får. Det fullständiga numret som e-legitimationen returnerade förblir krypterat i databasen. När du redigerar en part lämnar du fältet tomt för att behålla det lagrade numret.

## Legitimera för att visa och för att signera

**Legitimera för att visa** och **Legitimera för att signera** är två separata steg, även med samma e-legitimation. Bara en legitimering som görs för signering talar om för personen vad hon eller han signerar (BankID visar det i appen), så en part med båda legitimerar sig två gånger.

## SMS-koder

Parten ber om en kod på signeringssidan och får en sexsiffrig kod med SMS.

- En kod gäller i 10 minuter och tillåter 5 försök.
- Högst 5 koder i timmen kan skickas till en part.
- Koder lagras bara som en nyckelbaserad hash.
- Den förseglade PDF:en visar telefonnumret med bara de tre sista siffrorna.

### Ställa in SMS

Välj en leverantör i **Inställningar → Sms**:

| Leverantör | Användarnamn | Lösenord |
| --- | --- | --- |
| [46elks](https://46elks.com) | API-användarnamn | API-lösenord |
| [Twilio](https://www.twilio.com) | Account SID | Auth token |

**Avsändare** är ett namn på upp till 11 bokstäver, eller ett nummer som du äger. Samma leverantör används för leverans av signeringslänkar med SMS.

### Begränsa vad SMS kan kosta

Textmeddelanden kostar pengar, och delningslänkar låter personer du inte känner begära dem. De här inställningarna håller räkningen begränsad:

- **Tillåtna länder**: de landsnummer som får SMS, till exempel `46, 47`. Tomt tillåter alla länder, vilket är hur en installation börjar så att inget som redan fungerar slutar fungera. Ställ in de länder du faktiskt skickar till: vissa utländska nummer kostar många gånger mer än ett lokalt.
- **Meddelanden per timme** (200 som standard) och **Meddelanden per dag** (1000): för allt som installationen skickar. När en gräns nås avvisas fler meddelanden tills tidsfönstret har gått, och servern loggar en varning.
- **Meddelanden per timme för en delningslänk** (30): för alla dokument som gjorts från en delningslänk.

0 betyder ingen gräns.

## Svenskt BankID

Undertecknaren legitimerar sig med BankID-appen: på samma enhet med **Öppna BankID på den här enheten**, eller genom att skanna en QR-kod med **BankID på en annan enhet**. Om parten har ett personnummer kan bara den personens BankID svara.

Det finns två sätt att ansluta BankID. Parterna väljer "Svenskt BankID" oavsett vilket.

### Med ditt eget BankID-avtal

1. Slå på **Svenskt BankID** i **Inställningar → BankID och e-legitimation**.
2. För att prova det slår du på **Använd BankID:s testmiljö** och lämnar certifikatet tomt. OpenSign använder BankID:s publika testcertifikat, och du kan signera med ett [test-BankID](https://developers.bankid.com/test-portal).
3. För produktion stänger du av testmiljön och laddar upp det certifikat för förlitande part som din bank gav dig (en `.p12`-fil) tillsammans med dess lösenord.

OpenSign pratar med BankID över ömsesidig TLS och litar bara på BankID:s eget rotcertifikat, som är inbyggt. Ersätt det under **BankID-serverns CA** bara om BankID säger åt dig att göra det.

### Via TIC Identity

[TIC Identity](https://id.tic.io) erbjuder BankID utan eget avtal med en bank.

1. Slå på OpenID Connect för din klient i TIC:s portal och registrera `https://<your-install>/api/eid/callback` som omdirigerings-URI.
2. Slå på **BankID via TIC Identity** i **Inställningar → BankID och e-legitimation** och ange klientnamnet (som i `https://id.tic.io/<tenant>`), klient-ID och klienthemlighet.

TIC används så snart BankID med ditt eget certifikat är avstängt. OpenSign litar bara på det svenska personnumret i TIC:s `nin`-claim och kräver att det är giltigt.

## Freja eID

1. Slå på **Freja eID** i **Inställningar → BankID och e-legitimation**. Slå på **Använd Frejas testmiljö** medan du testar.
2. Ladda upp **Klientcertifikat** från Freja (en `.pfx`-fil) tillsammans med dess lösenord.
3. Lägg till **Frejas certifikat för signerade svar**. OpenSign avvisar varje svar vars signatur inte stämmer mot det.

Undertecknaren bekräftar i Freja-appen.

## Andra nordiska e-legitimationer via en förmedlare

En OpenID Connect-förmedlare som Idura (Criipto) eller Signicat ger tillgång till norskt och danskt BankID, MitID, finländska bank-ID och mer.

1. Skapa en applikation hos förmedlaren och registrera `https://<your-install>/api/eid/callback` som omdirigerings-URI.
2. Slå på **E-legitimationsförmedlare (OpenID Connect)** i **Inställningar → BankID och e-legitimation** och ange **Utfärdar-URL**, **Klient-ID** och **Klienthemlighet**.
3. Ställ eventuellt in **Val av e-legitimation (acr_values)** för att välja en e-legitimation, till exempel `urn:grn:authn:no:bankid`. Tomt låter undertecknaren välja hos förmedlaren.

Undertecknaren skickas till förmedlaren och tillbaka. OpenSign använder authorization code-flödet med PKCE, state och nonce, och kontrollerar ID-tokenens signatur, utfärdare, målgrupp och giltighetstid.

Legitimeringen måste slutföras i webbläsaren där den började: en kortlivad cookie knyter ihop de två, så detta gäller även för TIC Identity, och ett svar som kommer tillbaka någon annanstans avvisas och registreras i granskningsspåret. Låt cookies vara på för din OpenSign-adress.

När signeringssidan är inbäddad på en annan webbplats kan en cookie inte sättas inifrån ramen. Undertecknaren får då ett fönster på din OpenSign-adress som anger vilket dokument legitimeringen gäller, och går vidare till förmedlaren när knappen trycks. Detta erbjuds bara för parter med länkleverans, i en installation som tillåter inbäddning.

## Vad som registreras

För varje legitimering sparar OpenSign metoden, namnet och (maskerade) personnumret som e-legitimationen returnerade, tiden, IP-adressen och webbläsaren. Bevis från e-legitimation lagras krypterat. Det visas i granskningsspåret och på bevissidorna i den förseglade PDF:en. Se [Förseglade PDF:er och bevis](sealed-pdfs.md).
