# Snabbstart

Den här sidan tar dig från ingenting till ett signerat dokument på din egen dator. För en produktionsinstallation, läs [Driftsättning](deployment.md) härnäst.

## 1. Kör OpenSign lokalt

Du behöver Docker och en kopia av repositoryt.

```sh
git clone https://github.com/nielsbosma/OpenSign.git
cd OpenSign
docker compose up -d --wait postgres s3 mailpit converter
docker compose run --rm s3-init
docker compose --profile prod up -d --build
```

De två första kommandona startar Postgres, en S3-kompatibel lagring, Gotenberg för Word-filer och Mailpit, och skapar bucketarna. Det sista bygger och startar produktionsimagen.

- OpenSign: http://localhost:8090
- Mailpit, en lokal inkorg för utveckling: http://localhost:8026

Produktionsimagen skickar riktig e-post, så den behöver ett riktigt e-postkonto (steg 3). För att prova OpenSign utan ett sådant kör du i stället utvecklingsläge, där all e-post hamnar i Mailpit:

```sh
pnpm install
pnpm infra:up
cp .env.example .env
pnpm dev            # web app on http://localhost:5173
```

## 2. Skapa den första administratören

En ny installation skriver ut en **inställningskod** i sin logg, så att ingen annan kan ta över den före dig.

```sh
docker compose logs app | grep "setup code"
```

I utvecklingsläge finns den i terminalen där `pnpm dev` körs.

Öppna OpenSign och ange inställningskoden, ditt namn, din organisation, din e-postadress och ett lösenord. Se [Första inställningen och e-post](setup.md) för vad varje steg gör.

## 3. Anslut e-post

OpenSign skickar inbjudningar, koder och signerade dokument med e-post, så den ber om en e-postleverantör innan någon kan skicka. Ange en Resend API-nyckel och en avsändaradress på en domän som du har verifierat i Resend. OpenSign skickar ett testmejl till dig och går bara vidare när det har gått fram.

I utvecklingsläge kan du lämna nyckeln tom: testmejlet och allt som följer efter det går till Mailpit.

## 4. Skicka ett dokument

1. Gå till **Dokument** och välj **Nytt dokument**.
2. Släpp en PDF- eller Word-fil, ge den en titel och skapa den.
3. Lägg till dig själv som undertecknare med din e-postadress under **Parter**.
4. Välj parten på fliken **Fält** och placera ett **Underskrift**-fält på sidan.
5. Välj **Skicka för signering**.

Öppna inbjudan, följ länken, rita eller skriv en underskrift och signera. Några sekunder senare är dokumentet **Signerat**, och du kan ladda ner den förseglade PDF:en från dokumentsidan.

## 5. Kontrollera resultatet

Öppna **Verifiera ett dokument** i sidofältet (eller gå till `/verify`) och släpp den förseglade PDF:en. OpenSign kontrollerar sigillet och tidsstämplarna och bekräftar att det var just den här filen som förseglades. En PDF-läsare som förstår signaturer, till exempel Adobe Acrobat, visar också sigillet.

## Nästa steg

- [Driftsättning](deployment.md): kör det på riktigt, på Sliplane eller var som helst där containrar kan köras.
- [Legitimering](identification.md): ställ in SMS-koder, BankID eller Freja.
- [Användare och roller](users-and-roles.md): bjud in dina kollegor.
- [REST API](api.md): skicka dokument från dina egna system.
