# OpenSign-dokumentation

OpenSign är en e-signeringstjänst med öppen källkod som du driver själv. Du laddar upp ett Word- eller PDF-dokument, placerar fält och skickar det. Varje undertecknare får en länk, styrker vem hon eller han är på det sätt du valt och signerar. När alla är klara får du tillbaka en förseglad PDF som vilken PDF-läsare som helst kan verifiera, med bevissidor som visar vem som signerade, när och hur personen legitimerade sig.

Den här dokumentationen följer ditt språk, och språkmenyn högst upp ändrar det. Översättningarna är maskinöversatta och den engelska versionen är den som gäller.

De här sidorna täcker alla delar av OpenSign, för dig som skickar dokument, för administratörer som driftar en installation och för utvecklare som integrerar med tjänsten.

## Där du börjar

| Du vill | Läs |
| --- | --- |
| Prova OpenSign eller ta det i produktion | [Snabbstart](quick-start.md), därefter [Driftsättning](deployment.md) |
| Slutföra inställningen av en ny installation | [Första inställningen och e-post](setup.md) |
| Skicka ditt första dokument | [Dokument](documents.md) och [Parter, roller och signeringsordning](parties.md) |
| Be undertecknare använda BankID, Freja eller SMS-koder | [Legitimering](identification.md) |
| Skicka en mall till hundratals personer | [Massutskick](bulk-send.md) |
| Kontrollera att en signerad PDF är äkta | [Verifiera ett dokument](verification.md) |
| Koppla ihop ett CRM-system eller något annat system | [REST API](api.md) och [Webhooks](webhooks.md) |
| Spara signerade PDF:er i Google Drive, posta i Slack, uppdatera HubSpot | [Automatiseringar](automations.md) via Composio |
| Flytta från Scrive eller DocuSign | [Scrive-kompatibilitet](scrive.md) eller [DocuSign-kompatibilitet](docusign.md) |
| Arbeta med OpenSign från Claude | [MCP och Claude](mcp.md) |
| Förstå hur OpenSign skyddar dokument | [Säkerhetsmodell](security.md) |

## Vad OpenSign gör

- **Signeringsflöden.** Undertecknare, godkännare och läsare, i en bestämd ordning eller alla på en gång. Länkar skickas med e-post eller SMS, du lämnar över dem själv, eller så signerar undertecknaren på plats på din enhet. Påminnelser, utgångsdatum och möjligheten att avböja med en motivering ingår. Se [Parter, roller och signeringsordning](parties.md).
- **Legitimering.** En länk via e-post, en SMS-kod, svenskt BankID (med ditt eget bankavtal eller via TIC Identity), Freja eID eller andra nordiska e-legitimationer via en OpenID Connect-mäklare. Med ett personnummer på parten kan bara den personen signera. Se [Legitimering](identification.md).
- **Mallar och massutskick.** Spara ett förberett dokument som mall och skicka det sedan till varje rad i en CSV- eller Excel-fil. Se [Mallar](templates.md) och [Massutskick](bulk-send.md).
- **Bilagor från undertecknare.** Be en part om filer, till exempel en kopia av ett leg, som rensas och förseglas in i den signerade PDF:en. Se [Bilagor från undertecknare](attachments.md).
- **Förseglade PDF:er.** PAdES B-LTA-sigill med en RFC 3161-tidsstämpel och allt som behövs för att validera filen offline i många år. Se [Förseglade PDF:er och bevis](sealed-pdfs.md).
- **Mappar och arkiv.** Sortera dokument och mallar i delade mappar och arkivera färdigt arbete så att listorna hålls korta. Se [Mappar och arkiv](folders.md).
- **Gallring och rättslig spärr.** Radera dokument automatiskt efter bestämda tider och behåll de viktiga med en rättslig spärr. Se [Gallring och rättslig spärr](retention.md).
- **Användare och roller.** Administratör, ansvarig, avsändare, revisor och läsare, med passkeys eller en autentiseringsapp för tvåfaktorsinloggning. Se [Användare och roller](users-and-roles.md).
- **API:er.** Ett REST API med OpenAPI, Scrive- och DocuSign-slutpunkter som fungerar som direkta ersättare, signerade webhooks och en MCP-server för Claude och andra AI-klienter. Se [REST API](api.md).
- **AI, om du vill ha det.** En assistent och en granskning av dokument innan du skickar dem. Avstängt tills en administratör väljer en leverantör. Se [AI-assistent och granskning](ai.md).
- **Säkerhet först.** Kryptering i vila, ett manipuleringsskyddat granskningsspår, strikta säkerhetsrubriker för webben och behörighetskontroller på serversidan för varje anrop. Se [Säkerhetsmodell](security.md).

## Så hänger det ihop

OpenSign är en enda containerimage. Den behöver Postgres, en S3-kompatibel bucket och, om du vill ta emot Word-filer, en Gotenberg-konverterare i ett privat nätverk. Webbappen, API:erna, MCP-servern och bakgrundsarbetaren körs alla i samma image och använder samma tjänster, så en behörighet eller en regel gäller på samma sätt oavsett hur en förfrågan kommer in.

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Email, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

OpenSign har MIT-licens. Källkoden finns på [GitHub](https://github.com/nielsbosma/OpenSign).
