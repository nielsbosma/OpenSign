# Rapportera ett fel

Hittat något som inte fungerar? Rapportera det inifrån OpenSign. Din rapport blir ett offentligt ärende på GitHub, i ditt eget namn, så att de som bygger OpenSign kan se det du såg.

## Så rapporterar du

1. Öppna menyn under ditt namn, längst ned i sidofältet, och välj **Rapportera ett fel**.
2. En panel öppnas längst ned på sidan. Välj ett verktyg:
   - **Kommentar**: klicka på ett element, skriv vad som är fel med det och tryck på Enter. Det får en numrerad ruta.
   - **Dölj**: dra över allt privat för att svärta det.
   - **Pausa**: använd sidan som vanligt, till exempel för att först öppna en meny eller dialogruta. Esc pausar också.
3. Beskriv vad som gick fel och vad du väntade dig. En rubrik är valfri.
4. Tryck på **Granska rapporten**. Du ser den slutliga skärmbilden och varje rad som kommer att skickas. Ändra eller ta bort allt du inte vill dela, eller skippa skärmbilden.
5. Kryssa i rutan som bekräftar att rapporten får vara offentlig och tryck sedan på **Fortsätt på GitHub**.
6. Ett nytt fönster öppnas hos felrapporttjänsten. Det visar rapporten en gång till. Logga in med GitHub där och publicera den. Fönstret länkar sedan till ditt nya ärende.

Du behöver ett GitHub-konto som är minst en vecka gammalt. Om webbläsaren blockerar det nya fönstret visar OpenSign en länk så att du kan öppna det själv.

## Vad som publiceras

Ärendet visar:

- din rubrik och beskrivning, och dina kommentarer med det element som var och en gäller,
- skärmbilden, om du behöll den,
- sidan som ett mönster som `/documents/:id`, OpenSign-versionen, din webbläsare och ditt system, fönsterstorleken och appens språk,
- rubriker på öppna dialogrutor, senaste konsolfel och senaste misslyckade anrop, om du behöll de raderna.

Det visar aldrig adressen till din OpenSign, den verkliga sidadressen, ditt namn eller din e-postadress i OpenSign eller innehållet i ett dokument. Tekniska rader blir av med e-postadresser, webbadresser, id:n, tokens, nycklar och långa nummer innan de lämnar OpenSign; vart och ett ersätts med `[removed]`.

Ärendet skapas av ditt GitHub-konto, med märket för appen **OpenDeploy Bug Reports**, som bara får öppna ärenden. Tjänsten använder din inloggning för just den förfrågan och återkallar den sedan.

## Vad som täcks i skärmbilden

Innan skärmbilden tas täcker OpenSign över med grått:

- allt som skrivits i ett fält,
- PDF-sidor och miniatyrer,
- namn och e-postadresser för parter och användare,
- dokumenttitlar,
- poster i granskningsloggen,
- webhookadresser och tillåtna IP-adresser för API-nycklar.

De svarta rutorna du ritar med **Dölj** bränns också in i bilden. Ingenting på själva sidan ändras. Kontrollera skärmbilden i granskningssteget: allt annat privat är ditt ansvar att dölja.

## Ta bort en skärmbild

Skärmbilder är offentliga och sparas tills någon tar bort dem. Varje ärende slutar med **Ta bort den här skärmbilden**. Öppna den, logga in med GitHub, så raderas bilden och ärendet säger det. Du kan ta bort dina egna skärmbilder; repots förvaltare kan ta bort vilken som helst.

GitHub sparar cachade kopior av bilder en tid, och vem som helst kan ha sparat den, så att ta bort en skärmbild kan inte ta tillbaka varje kopia. Därför ber granskningssteget dig att kontrollera att inget privat syns på skärmen.

## Vad som lämnar din OpenSign

Ingenting skickas förrän du trycker på **Fortsätt på GitHub**. Då skickar din OpenSigns server rapporten till felrapporttjänsten på `sliplane.opendeploy.cloud`, som sparar den i högst en timme tills du publicerar den. OpenSign har inga inloggningsuppgifter till GitHub.

Varje person kan skicka 10 rapporter i timmen, och en OpenSign 30 i timmen.

## För administratörer

Felrapporter är på som standard. Under **Inställningar → Felrapporter** kan en administratör stänga av dem, vilket döljer **Rapportera ett fel** för alla, eller skicka rapporter till ett annat repo, till exempel en fork. Se [Administratörsinställningar](settings.md#bug-reports). Varje skickad rapport registreras i granskningsloggen.

En installation utan utgående internetåtkomst kan inte skicka rapporter.
