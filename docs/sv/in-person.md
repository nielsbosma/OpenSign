# Signering på plats

Signering på plats låter en part signera på din egen dator eller surfplatta, till exempel vid en disk, under ett möte eller hemma hos en kund. Parten behöver varken e-postadress eller telefon.

## Ställa in det

I partdialogen ställer du in **Skicka länk via** på **På plats (på din enhet)**. OpenSign skickar då ingenting till den här parten. Du kan fortfarande be parten legitimera sig med en SMS-kod eller en e-legitimation; parten gör det på din enhet.

## Signera

1. Skicka dokumentet som vanligt.
2. När det är partens tur öppnar du dokumentet och väljer **Signera på plats** på partens kort.
3. OpenSign öppnar signeringssidan med en överlämningsskärm: "Lämna enheten till …". Den förklarar att allt parten gör registreras i dokumentets bevis.
4. Parten bekräftar med **Jag är …**, läser dokumentet, legitimerar sig om det krävs och signerar.
5. När parten är klar ber sidan om att enheten lämnas tillbaka till dig.

## Vad som registreras

- När du startar signering på plats utfärdas en ny länk för parten och alla tidigare länkar dras in, så en länk som du startade tidigare kan inte användas igen.
- Granskningsspåret registrerar vem som startade sessionen på plats och för vilken part. Bevissidorna visar partens legitimering, IP-adress och webbläsare, som för vilken annan undertecknare som helst.

## Signera ett dokument själv

För ett dokument som bara du behöver signera, till exempel en försäkran, en orderbekräftelse eller ett formulär som du fyller i för dina egna papper, väljer du **Signera själv** på utkastet i stället för **Skicka**.

1. Välj hur du legitimerar dig: **Min inloggning i OpenSign** (standard), eller en e-legitimation som BankID när en administratör har ställt in en.
2. Välj **Signera nu**. Du blir ensam undertecknare: undertecknare och godkännare som redan finns på utkastet tas bort och deras fält blir dina. De som får en kopia fortsätter att få en.
3. OpenSign skickar dokumentet och öppnar signeringssidan på din enhet, utan någon överlämningsskärm. Din [sparade underskrift](account-security.md#saved-signature) erbjuds om du har en.
4. När du signerar förseglas dokumentet och kan verifieras som vilket annat dokument som helst.

Med **Min inloggning i OpenSign** står det på bevissidorna "Identified by: Signed-in OpenSign account" med ditt namn och din e-postadress, och granskningsspåret registrerar användaren som startade det. Om du lämnar sidan innan du signerat öppnar **Signera på plats** på ditt partskort signeringssidan igen.

## Anmärkningar

- **Signera på plats** erbjuds bara för parter vars leverans är På plats, medan dokumentet väntar och det är den partens tur.
- Att starta signering på plats, eller signera ett dokument själv, kräver en inloggad användare. Ingetdera är tillgängligt med en API-nyckel eller via MCP.
- Scrives leverans `pad` motsvarar signering på plats. Se [Scrive-kompatibilitet](scrive.md).
