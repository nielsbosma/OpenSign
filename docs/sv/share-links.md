# Delningslänkar och QR-koder

En delningslänk låter vem som helst starta ett dokument från en mall på egen hand: ett medlemsformulär på din webbplats, en friskrivning i receptionen, ett samtyckesformulär utskrivet som QR-kod på en affisch. Den som öppnar länken anger sitt namn och sin e-postadress, bekräftar e-postadressen med en kod och signerar sedan sin egen kopia. Inget skapas för en adress som inte har bekräftats.

## Slå på en delningslänk

1. Öppna mallen. Du måste kunna ändra den och skicka dokument.
2. Slå på **Dela med en länk eller QR-kod** under **Publik länk**.
3. I **Fylls i av besökaren** väljer du den mallpart som den som öppnar länken blir, till exempel "Kund". Parten måste signera eller godkänna. Alla andra parter förblir som i mallen, så en motsignerare från din sida signerar fortfarande efter besökaren.
4. Kopiera länken, eller välj **QR-kod** och ladda ner den som PNG (för skärmar och dokument) eller SVG (för tryck).

Länken ser ut så här: `https://sign.example.com/s/Xq3…`. Den är avstängd tills du slår på den, och när du stänger av den slutar den fungera direkt. Dokument som skapas via länken skickas från den användare som slog på den och visas i den användarens dokumentlista, med en titel som bygger på mallen och besökarens namn.

Delningslänkar kräver att e-post är inställd, eftersom varje besökare bekräftar sin adress. Se [Inställningar](settings.md).

## Vad besökaren ser

1. En startsida med din logotyp, mallens titel och meddelande, på mallens språk. Besökaren kan byta till vilket som helst av de [11 signeringsspråken](signing-page.md#languages).
2. Besökaren anger sitt namn och sin e-postadress (och telefonnummer, när parten får SMS-koder eller SMS-länkar). Ett namn får bara innehålla bokstäver, inte siffror eller adresser, eftersom det hamnar i dokumentets titel och i e-postmeddelandena om det. Textmeddelanden för en länks dokument räknas mot SMS-gränserna i [Legitimering](identification.md).
3. OpenSign mejlar en sexsiffrig kod som gäller i 15 minuter. Besökaren anger den.
4. OpenSign gör ett dokument av mallen med besökaren som part och skickar det. Signeringssidan öppnas direkt; samma länk mejlas också, så att besökaren kan återvända till den senare.

Om andra parter signerar före besökaren i ett dokument som signeras i ordning får besökaren veta att ett e-postmeddelande kommer när det är dennes tur.

Den legitimering som är inställd på mallparten gäller fortfarande. Om parten måste legitimera sig med BankID eller en SMS-kod gör besökaren det på signeringssidan, som vilken undertecknare som helst. Se [Legitimering](identification.md).

## Gränser

Varje länk har gränser som du ställer in under **Publik länk**:

| Gräns | Vad den gör |
| --- | --- |
| **Per timme** | Högst så här många dokument per timme (1 till 1000, standard 10). |
| **Totalt** | Högst så här många dokument sammanlagt. Lämna tomt för ingen gräns. |
| **Sista dag** | Länken slutar fungera efter den här dagen. |

Utöver dessa skyddar OpenSign varje länk på egen hand:

- Varje adress får högst 3 koder i timmen, och varje länk högst tre gånger sin timgräns (minst 20) i koder, så att en länk inte kan användas för att översvämma någons inkorg.
- En kod tillåter 5 försök och används en gång.
- Varje IP-adress hastighetsbegränsas på startsidan och när koder begärs eller anges, som på signeringssidan.
- Obekräftade förfrågningar raderas efter en dag.

## Byta adress

**Ny adress** ger länken en ny adress. Den gamla adressen och alla QR-koder som skrivits ut från den slutar fungera direkt. Använd det när en länk har delats vidare längre än du avsåg.

## Med API:et

- `GET /api/v1/templates/{templateId}/share-link`: länken, dess gränser och hur många dokument den har gjort.
- `PUT /api/v1/templates/{templateId}/share-link` med `enabled`, `partyId`, `perHour`, `maxTotal` och `expiresAt`.
- `POST /api/v1/templates/{templateId}/share-link/rotate`: en ny adress.

De är också MCP-verktyg (`share_links_get`, `share_links_set`, `share_links_rotate`). Se [REST API](api.md) och [MCP](mcp.md).

Dokument som skapas via en delningslänk utlöser `document.created` med `"source": "share_link"` och länkens `shareLinkId`. Se [Webhooks](webhooks.md#events).

## Vad som registreras

Mallens granskningsspår registrerar varje ändring av länken, varje kod som skickats (med e-postadressen) och varje dokument som skapats. Det nya dokumentets granskningsspår börjar med att det skapades från delningslänken.
