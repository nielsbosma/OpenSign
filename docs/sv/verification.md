# Verifiera ett dokument

Vem som helst som har en signerad PDF kan kontrollera att den är äkta och oförändrad. Inget konto behövs.

## På verifieringssidan

Öppna `https://<your-install>/verify` (länkas också som **Verifiera ett dokument** i appens sidofält, och är utskriven på bevissidorna i varje förseglat dokument) och släpp PDF:en.

Filen kontrolleras på servern och lagras inte. Sidan svarar på två frågor:

1. **Förseglade den här installationen exakt den här filen?** "Förseglat av denna OpenSign" med dokumentets titel och klartid betyder att filens SHA-256 stämmer med ett dokument som den här servern förseglade och att dess sigill håller, gjort med ett av den här serverns sigillcertifikat. "Inte ett dokument som den här servern har förseglat" betyder att den ändrades efter förseglingen, eller att den kommer från någon annanstans.
2. **Är signaturerna hela?** Varje signatur och tidsstämpel i filen listas med vem som gjorde den, när och om den är hel. Hel betyder bara att byten är som de signerades: vem som helst kan göra ett certifikat med vilket namn som helst i. En signatur som är hel men inte gjord av den här servern säger det, och namnet på den är obekräftat.

## Vad kontrollen omfattar

För varje signatur och dokumenttidsstämpel i filen kontrollerar OpenSign:

- att signaturens kryptografi är giltig och täcker de byte den påstår sig täcka,
- om den täcker hela filen, eller bara en tidigare revision,
- om allt som lagts till efter den bara är valideringsdata: ett dokumentsäkerhetslager med certifikat och spärrsvar, samt osynliga dokumenttidsstämplar,
- tidsstämpelutfärdarens egen signatur (RSA på 2048 bitar eller mer, ECDSA P-256 och P-384, SHA-2), att dess certifikat är avsett för tidsstämpling och att det var giltigt vid tidpunkten som stämplades. En tidsstämpel som inte klarar detta visar ingen tid.

Varje annan ändring efter en signatur, till exempel en ny anteckning, en formulärredigering, en ändrad katalogpost, ett raderat objekt eller ett nytt objekt som inget verkar använda, markerar den signaturen som inte hel. Det gör också en uppdatering som inte är skriven på det enkla sätt som OpenSign skriver sina egna. Det stänger det vanliga tricket att ändra en signerad PDF med en inkrementell uppdatering som vissa läsare inte flaggar, och det innebär att en fil som ett annat verktyg har lagt till valideringsdata i kan visas som ändrad.

OpenSign kontrollerar inte undertecknare eller tidsstämpelutfärdare mot en förtroendelista. För det, använd en PDF-läsare som har en.

## I en PDF-läsare

Förseglade dokument är standardiserade PAdES-filer, så Adobe Acrobat, Foxit och andra läsare som förstår signaturer visar sigillet, tidsstämpeln och om dokumentet har ändrats. Eftersom sigillcertifikatet som standard är självsignerat kan en läsare säga att den inte känner till undertecknarens identitet; verifieringssidan ovan är den auktoritativa kontrollen för din installation. Se [Förseglade PDF:er och bevis](sealed-pdfs.md#the-seal-certificate).

## Från API:et

`POST /api/v1/verify` med den råa PDF:en som body (upp till 40 MB) returnerar samma resultat som JSON. Det behöver ingen API-nyckel och hastighetsbegränsas per adress. Se [REST API](api.md).

```sh
curl -X POST https://sign.example.com/api/v1/verify \
  -H "Content-Type: application/pdf" --data-binary @signed.pdf
```
