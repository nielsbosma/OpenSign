# Inloggning, passkeys och tvåfaktor

Ditt konto skyddas av ett lösenord och helst en andra faktor: en autentiseringsapp eller en passkey. Hantera båda under **Inställningar → Konto**.

## Lösenord

- Minst 12 tecken. Långa lösenfraser är välkomna.
- Lösenord hashas med Argon2id. Ingen, inte ens administratörer, kan läsa dem.
- Efter upprepade misslyckade inloggningar från en adress spärras den adressen för det här kontot en tid, från en minut och växande till en timme. En dag utan misslyckanden glömmer dem. Bara misslyckanden från flera adresser samtidigt kan låsa kontot för alla, och en passkey loggar in även då. Varje försök registreras.
- **Byt lösenord** frågar efter det nuvarande och loggar ut dina andra sessioner.
- **Glömt lösenordet?** på inloggningssidan mejlar en återställningslänk som gäller i en timme. En återställning loggar ut alla sessioner och återkallar dina API-nycklar, och du får ett e-postmeddelande om några har återkallats. Om ditt konto har en passkey behöver du den fortfarande för att logga in efteråt.

## Tvåfaktor med en autentiseringsapp

1. Starta inställningen under **Tvåfaktorsautentisering** och skanna QR-koden med en autentiseringsapp, till exempel 1Password, Google Authenticator eller Microsoft Authenticator.
2. Ange den sexsiffriga koden för att bekräfta.
3. Spara **återställningskoderna**. Var och en loggar in dig en gång om du tappar bort telefonen. De visas bara den här enda gången.

Därefter ber inloggningen om en kod efter ditt lösenord. Om du tappar bort både telefonen och återställningskoderna kan en administratör återställa din tvåfaktor (se [Användare och roller](users-and-roles.md)).

## Passkeys

Med en passkey kan du logga in med Face ID, Touch ID, Windows Hello, din telefon eller en säkerhetsnyckel, utan att skriva ett lösenord.

1. Under **Passkeys** väljer du **Lägg till en nyckel (passkey)**, ger den ett namn (till exempel "MacBook") och bekräftar ditt lösenord.
2. Följ webbläsarens anvisning.

Välj sedan **Logga in med passkey** på inloggningssidan. En passkey verifierar dig alltid på enheten (med fingeravtryck, ansikte eller PIN), så den räknas som tvåfaktor i sig och ingen kod efterfrågas. Du kan lägga till flera och ta bort vilken som helst av dem.

När ditt konto har en passkey och ingen autentiseringsapp räcker inte längre bara lösenordet för att logga in: efter lösenordet ombeds du också använda passkeyn. Om du förlorar alla passkeys kan en administratör återställa din tvåfaktor, vilket tar bort dem.

## Bekräfta att det är du

Att skapa en API-nyckel eller ansluta en app ger bestående åtkomst, så OpenSign ber först om ditt lösenord eller en passkey, om du inte loggade in eller bekräftade under de senaste tio minuterna.

## Kräva tvåfaktor för alla

En administratör kan slå på **Kräv tvåfaktorsautentisering för alla** i **Inställningar → Säkerhet**. Alla som saknar en autentiseringsapp eller en passkey ombeds då att ställa in en innan de kan göra något annat. MCP-klienter som är inloggade som ett sådant konto avvisas tills de har gjort det. Vi rekommenderar att du slår på detta.

## Enkel inloggning

Om din organisation använder [enkel inloggning](single-sign-on.md) väljer du dess knapp på inloggningssidan. Din identitetsleverantör hanterar då ditt lösenord, din andra faktor och din e-post, och **Inställningar → Konto** har inga passkeys, ingen autentiseringsapp och inget lösenord för dig. API-nycklar och MCP-klienter är inte tillgängliga för de här kontona.

## Språk

Under **Inställningar → Konto och säkerhet** väljer du det språk som OpenSign visas på för dig: engelska, kinesiska (förenklad), hindi, spanska, franska, arabiska, bengali, portugisiska, ryska, urdu eller svenska. Före inloggningen följer appen din webbläsare, och inloggningssidan har en språkväljare. Detta ändrar inte vad undertecknare ser; de får det språk som är inställt på deras dokument eller part. Vissa meddelanden från servern är fortfarande på engelska.

## Sparad underskrift

Under **Inställningar → Konto och säkerhet → Min signatur** ritar eller skriver du din underskrift en gång, och lägger till dina initialer om du vill. När du signerar ett dokument själv erbjuder signeringssidan den på fliken **Sparad**. Den tillämpas aldrig av sig själv: du väljer fortfarande att signera, och du kan rita eller skriva en ny i stället.

Den erbjuds bara när signeringslänken tillhör dig: när du [signerar ett dokument själv](in-person.md#signing-a-document-yourself), eller öppnar en länk som skickats till din egen e-postadress medan du är inloggad. Den som du lämnar din enhet till för signering på plats ser den aldrig. Teckningen lagras krypterat som varje annan fil, och **Ta bort** raderar den. Att spara och ta bort den registreras i granskningsspåret.

Externa undertecknare har inget konto, så de kan i stället kryssa i **Kom ihåg min signatur i den här webbläsaren**. Det behåller den bara i deras webbläsare, och erbjuder den nästa gång till en part med samma namn; ingenting om dem lagras på servern mellan dokument. Se [Signeringssidan](signing-page.md#signing).

## Sessioner

Sessioner är webbläsarcookies som skript inte kan läsa, bundna till din installation och lagrade på servern bara som en nyckelbaserad hash. I **Inställningar → Säkerhet** ställer en administratör in:

| Inställning | Standard | |
| --- | --- | --- |
| Sessionslängd (timmar) | 12 | Hur länge en inloggning varar som längst. |
| Logga ut efter inaktivitet (minuter) | 60 | Hur länge en session överlever utan användning. |

**Logga ut** i användarmenyn avslutar sessionen i den här webbläsaren.

## Appar anslutna via MCP

Under **AI-appar (MCP)** ser du MCP-serverns adress för din installation och de klienter du har anslutit, till exempel Claude. Koppla bort vilken som helst av dem när som helst. Se [MCP och Claude](mcp.md).

## API-nycklar

API-nycklar hanteras under **Inställningar → API-nycklar**. Se [REST API](api.md#api-keys).
