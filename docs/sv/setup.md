# Första inställningen och e-post

En ny installation leder dig genom två steg innan någon kan använda den: skapa den första administratören och anslut sedan e-post. Tills båda är klara skickar OpenSign alla administratörer tillbaka till inställningssidan.

## Steg 1: den första administratören

När OpenSign startar och inte har några användare skriver den ut en engångs-**inställningskod** i sin logg:

```
No admin yet. Open https://sign.example.com and enter the setup code …
```

Öppna installationen i en webbläsare och fyll i:

| Fält | |
| --- | --- |
| Inställningskod | Koden från loggen. Den visar att du har åtkomst till servern, så att ingen som hittar en ny installation på internet kan ta över den. |
| Organisation | Din organisations namn. Undertecknare ser det i e-post och på signeringssidan. Du kan ändra det senare i **Inställningar → Organisation**. |
| Ditt namn, E-post, Lösenord | Ditt administratörskonto. |

Installationer från knappen **Deploy to Sliplane** hoppar över koden: driftsättaren genererar en `SETUP_CODE`, och **Öppna OpenSign** på dess avslutande sida går till `/setup#code=…`, som fyller i den. Koden ligger i URL-fragmentet, som webbläsaren aldrig skickar till en server, och sidan tar bort den från adressfältet. Om du öppnar installationen på något annat sätt visas koden på driftsättarens avslutande sida och i tjänstens miljö i Sliplane. När `SETUP_CODE` är satt skriver loggen inte längre ut någon kod.

När den första administratören finns slutar inställningskoden att fungera för gott. Alla andra bjuds in från **Inställningar → Användare** (se [Användare och roller](users-and-roles.md)).

## Steg 2: e-post

OpenSign skickar signeringsinbjudningar, påminnelser, meddelanden när ett dokument har stoppats, de förseglade dokumenten, användarinbjudningar och lösenordsåterställningar med e-post. Inställningen kräver en fungerande e-postleverantör, Resend eller din egen SMTP-server, och skickar ett testmeddelande till dig. Inställningen blir klar först när leverantören har godtagit det, och dokument kan inte skickas med e-post före dess.

### Resend

[Resend](https://resend.com) är standardleverantören.

1. Skapa ett Resend-konto och verifiera den domän du vill skicka från under **Domains**.
2. Skapa en API-nyckel under **API Keys**, med behörighet att skicka.
3. Ange i OpenSign API-nyckeln, en **Avsändaradress** på den verifierade domänen (till exempel `sign@example.com`) och ett **Avsändarnamn**.

### SMTP

Välj fliken **SMTP-server** under inställningen, eller **SMTP** som **Leverantör** i **Inställningar → E-post**, för att skicka via din egen e-postserver eller en annan leverantörs SMTP-relä.

| Fält | |
| --- | --- |
| SMTP-server | Värdnamnet, till exempel `smtp.example.com`. Utan `smtp://` och utan port. |
| Port | Oftast 587 för STARTTLS eller 465 för TLS. |
| Säkerhet | **STARTTLS** eller **TLS**. |
| Användarnamn, Lösenord | Ett tomt användarnamn skickar utan att logga in. Lösenordet lagras krypterat. |
| Avsändaradress, Avsändarnamn | Avsändaren. |

TLS krävs alltid och certifikatet måste gälla för servernamnet, så att inloggningsuppgifter aldrig skickas okrypterade. Det finns inget alternativ med oformaterad text. Servrar i privata nätverk avvisas, utom i utvecklingsläge.

### Ändra e-post senare

E-postinställningarna finns i **Inställningar → E-post**. Om du ändrar leverantör, nyckel, SMTP-server eller avsändare tas statusen "verifierad" bort, och OpenSign ber om ett nytt testmejl innan dokument kan skickas igen. **Svarsadress** är valfri: om den är tom går svaren till den som skickade respektive dokument.

## Vad du ställer in härnäst

All annan konfiguration görs i webbläsaren, under **Inställningar**. Inget av det behövs för att börja skicka dokument med e-postlänkar.

- **Organisation**: namn, organisationsnummer, land, tidszon, standardspråk för undertecknare, profilfärg och logotyp. Se [Administratörsinställningar](settings.md).
- **SMS**: för SMS-inbjudningar och engångskoder. Se [Legitimering](identification.md#sms-codes).
- **BankID och e-legitimation**: svenskt BankID, Freja eID och andra nordiska e-legitimationer. Se [Legitimering](identification.md).
- **Signering och sigill**: sigillets namn, orsak och tidsstämpelutfärdare. Se [Förseglade PDF:er och bevis](sealed-pdfs.md).
- **Säkerhet**: kräv tvåfaktorsinloggning för alla. Se [Inloggning, passkeys och tvåfaktor](account-security.md).
- **AI**: valfritt. Se [AI-assistent och granskning](ai.md).

## Fäst inställningar med miljövariabler

Alla administratörsinställningar kan sättas med en miljövariabel som heter `OPENSIGN_<GROUP>_<FIELD>`, till exempel `OPENSIGN_EMAIL_API_KEY` eller `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR=true`. Ett fastsatt fält är låst i webbläsaren och visar "Satt av en miljövariabel." Det är praktiskt för infrastruktur som kod, och för att hålla en hemlighet helt utanför databasen. Se [Administratörsinställningar](settings.md#pinning-settings) för den fullständiga listan över namn.
