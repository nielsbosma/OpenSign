# Texttaggar, formulärimport och fältidentifiering

Du behöver inte dra varje fält till sidan. OpenSign kan läsa fält från själva filen på tre sätt:

- **Texttaggar** som du skriver in i en Word-fil eller PDF, till exempel `{{Tenant Sign;role=Tenant;type=signature}}`.
- **PDF-formulär**: fälten i en ifyllbar PDF blir OpenSign-fält, med de värden de var ifyllda med.
- **Hitta fält** i redigeraren föreslår fält där sidan har linjer, understrykningar eller rutor att fylla i.

Fält som skapats på något av de här sätten är vanliga fält. Du kan flytta, ändra storlek på, ändra eller ta bort dem innan du skickar.

## Texttaggar

Skriv en tagg där ett fält ska sitta, i dokumentets text. När du laddar upp filen gör OpenSign varje tagg till ett fält på taggens plats och tar bort taggens text från sidan, så att undertecknare aldrig ser den. Resten av raden stannar exakt där den var.

Syntaxen är densamma som i DocuSeal, så mallar gjorda för DocuSeal fungerar oförändrade:

```
{{Field Name;role=Tenant;type=signature;required=false}}
```

Den första delen är fältets namn, som visas som dess etikett. Efter den kommer attribut av typen `key=value`, åtskilda av semikolon. En tagg kan utelämna namnet och börja med ett attribut, till exempel `{{type=checkbox}}`.

| Attribut | |
| --- | --- |
| `type` | Typen av fält, se nedan. `text` om det utelämnas. |
| `role` | Parten som fyller i det, med namn, till exempel `Tenant` eller `Landlord`. En roll som dokumentet inte har ännu läggs till som en ny part; fyll i partens uppgifter innan du skickar. Utan roll går fältet till den första part som signerar. |
| `required` | `false` gör fältet valfritt. Fält är obligatoriska som standard, kryssrutor är det inte. |
| `readonly` | `true` visar det förvalda värdet för parten utan att låta parten ändra det. |
| `default` | Ett värde som parten utgår från. |
| `title` | En annan etikett än namnet. |
| `options` | Alternativen i ett `select`- eller `multiple`-fält, åtskilda med kommatecken (eller med `|`). |
| `option` | Det enda alternativ som en `radio`-knapp står för. Radiotaggar med samma namn bildar en grupp. |
| `format` | Ett datumformat som `DD/MM/YYYY` eller `MM/DD/YYYY`, eller ett talformat: `usd`, `eur`, `gbp`, `sek`, eller hur 1234 skulle se ut, till exempel `1 234,00 kr`. Se [Tal- och datumformat](documents.md#number-and-date-formats). |
| `min`, `max` | Det minsta och största talet, eller det tidigaste och senaste datumet som YYYY-MM-DD. |
| `width`, `height` | Fältets storlek i punkter (1/72 tum). Utan dem är fältet lika brett som taggen och lika högt som dess text, och underskrifter, initialer och bilder får ett rimligt minimum, som växer uppåt från linjen. |

| Typ | Fält |
| --- | --- |
| `text` | Text |
| `signature` | Underskrift |
| `initials` | Initialer |
| `date` | Datumväljare |
| `datenow` | Signeringsdatum, fylls i automatiskt |
| `number` | Nummer |
| `select` | Rullgardinsmeny |
| `multiple` | Flerval |
| `radio` | En radioknapp |
| `checkbox` | Kryssruta |
| `phone` | Telefon |
| `image`, `file` | Uppladdning av bild eller fil |
| `name`, `email`, `company` | Fylls i från partens uppgifter (endast OpenSign) |

Exempel:

```
{{Client Name;role=Client}}
{{Client Sign;type=signature;role=Client}}
{{Start Date;type=date;format=DD/MM/YYYY;role=Client}}
{{Region;type=select;options=North,South,East,West}}
{{Plan;type=radio;option=Basic}} Basic   {{Plan;type=radio;option=Plus}} Plus
{{type=checkbox;role=Client}} I accept the terms
{{Amount;type=number;format=sek;min=0}}
```

En tagg som inte går att använda lämnas kvar på sidan som text, och OpenSign berättar varför när du laddar upp: en typ som OpenSign inte har (till exempel DocuSeals `payment`, `stamp` eller `verification`), ett attribut utan värde, ett värde som fältet inte godtar, eller en tagg som är bruten över två rader. Attribut som OpenSign inte har någon användning för, till exempel typsnitt och färger, ignoreras.

### Skriva taggar i Word

Skriv taggar som vanlig text. Håll varje tagg på en rad: korta ned namnet eller gör teckenstorleken mindre om Word bryter raden. Taggens teckenstorlek avgör fältets höjd, så en tagg i 8 punkters text ger ett lågt fält. Texttaggar fungerar i Word-, OpenDocument- och RTF-filer i installationer med en dokumentkonverterare, och i alla PDF:er vars text går att läsa. Inskannade sidor har ingen text, så de har inga taggar.

### Stänga av taggar

Taggar läses vid varje uppladdning. För att ladda upp en fil vars text råkar innehålla `{{` och `}}` stänger du av det via API:et med `textTags=false`. API:erna för [Scrive](scrive.md) och [DocuSign](docusign.md) läser aldrig taggar.

## PDF-formulär

Ett ifyllbart PDF-formulär (en AcroForm) blir fält när du laddar upp det: textrutor blir textfält, kryssrutor kryssrutor, radioknappar en radiogrupp, listor rullgardinsmenyer eller flerval och signaturrutor underskriftsfält. Värden som formuläret var ifyllt med blir förvalda värden, och fält som är markerade som obligatoriska i formuläret förblir obligatoriska.

Ett formulärfält vars namn börjar med en parts namn går till den parten: `Tenant.Name` och `Tenant name` går till Tenant. De övriga går till den första part som signerar. Själva formuläret tas ändå bort från filen, som vid varje uppladdning, så att bara OpenSigns fält finns kvar. XFA-formulär läses inte.

## Hitta fält

**Hitta fält** på fliken **Fält** tittar igenom sidorna efter ställen som vill ha ett fält:

- linjer och rader av understrykningar att skriva på, med etiketten bredvid eller under dem,
- tomma rutor att kryssa i, med texten bredvid som etikett.

Etiketten avgör typen: Signature, Underskrift eller Firma blir ett underskriftsfält, Name eller Namnförtydligande ett namn, Date eller Datum signeringsdatumet, Date of birth en datumväljare, Email, Phone och Company sina respektive fält, och allt annat text.

Förslagen ritas med streckade kanter och ingenting sparas förrän du godkänner dem. Klicka på ett för att lägga till det, eller på **Godkänn alla**. De går till parten som valts på fliken **Fält**; underskrifter och initialer går till en undertecknare. Håll muspekaren över ett förslag och klicka på krysset för att avfärda det. Ställen som redan har ett fält föreslås inte igen.

### Med AI

När en [AI-leverantör](ai.md) är inställd och **Tillåt att dokumenttext skickas för granskning** är på erbjuder Hitta fält också **Även med AI**. OpenSign skickar då modellen varje sidas text med dess positioner, samt de ställen den redan hittat, och lägger till modellens förslag markerade med en gnista. Den skickar inga bilder, högst 30 sidor och omkring 40 000 tecken, och anropet räknas mot AI-budgeten. Varje AI-identifiering skrivs till [granskningsloggen](audit-log.md).

## Via API:et

`POST /api/v1/documents/{id}/files` och `documents.upload` läser texttaggar och formulärfält om inte `textTags` eller `formFields` är `false`. Svarets `fields` anger hur många fält som skapades från vardera, vilka parter som lades till och vilka taggar som inte gick att använda.

`documents.detect_fields` returnerar förslag för ett utkast utan att spara något. Skicka `ai: true` för att fråga AI-modellen också. Placera de du vill ha med `documents.set_fields`. Se [API](api.md).
