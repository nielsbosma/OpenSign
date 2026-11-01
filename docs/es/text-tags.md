# Etiquetas de texto, importación de formularios y detección de campos

No tiene que arrastrar cada campo a la página. OpenSign puede leer los campos del propio archivo de tres maneras:

- **Etiquetas de texto** que escribe en un archivo Word o PDF, como `{{Tenant Sign;role=Tenant;type=signature}}`.
- **Formularios PDF**: los campos de un PDF rellenable se convierten en campos de OpenSign, con los valores con los que se rellenaron.
- **Detectar campos** en el editor sugiere campos donde la página tiene líneas, guiones bajos o recuadros por rellenar.

Los campos creados de cualquiera de estas maneras son campos normales. Puede moverlos, cambiarles el tamaño, modificarlos o quitarlos antes de enviar.

## Etiquetas de texto

Escriba una etiqueta donde deba ir un campo, en el texto del documento. Al subir el archivo, OpenSign convierte cada etiqueta en un campo en el lugar de la etiqueta y quita el texto de la etiqueta de la página, de modo que los firmantes nunca la ven. El resto de la línea queda exactamente donde estaba.

La sintaxis es la misma que la de DocuSeal, así que las plantillas hechas para DocuSeal funcionan sin cambios:

```
{{Field Name;role=Tenant;type=signature;required=false}}
```

La primera parte es el nombre del campo, que se muestra como su etiqueta. Después vienen atributos `key=value` separados por punto y coma. Una etiqueta puede omitir el nombre y empezar con un atributo, como `{{type=checkbox}}`.

| Atributo | |
| --- | --- |
| `type` | El tipo de campo, véase más abajo. `text` si se omite. |
| `role` | La parte que lo rellena, por nombre, como `Tenant` o `Landlord`. Un rol que el documento aún no tiene se añade como una parte nueva; rellene sus datos antes de enviar. Sin rol, el campo va a la primera parte que firma. |
| `required` | `false` hace el campo opcional. Los campos son obligatorios por defecto; las casillas no. |
| `readonly` | `true` muestra el valor por defecto a la parte sin dejarla cambiarlo. |
| `default` | Un valor del que parte la parte. |
| `title` | Una etiqueta distinta del nombre. |
| `options` | Las opciones de un campo `select` o `multiple`, separadas por comas (o por `|`). |
| `option` | La única opción que representa un botón `radio`. Las etiquetas radio con el mismo nombre forman un grupo. |
| `format` | Un formato de fecha como `DD/MM/YYYY` o `MM/DD/YYYY`, o un formato de número: `usd`, `eur`, `gbp`, `sek`, o cómo se vería 1234, como `1 234,00 kr`. Consulte [Formatos de número y de fecha](documents.md#number-and-date-formats). |
| `min`, `max` | El número mínimo y máximo, o la fecha más temprana y más tardía como YYYY-MM-DD. |
| `width`, `height` | El tamaño del campo en puntos (1/72 de pulgada). Sin ellos, el campo es tan ancho como la etiqueta y tan alto como su texto, y las firmas, las iniciales y las imágenes reciben un mínimo razonable, creciendo hacia arriba desde la línea. |

| Tipo | Campo |
| --- | --- |
| `text` | Texto |
| `signature` | Firma |
| `initials` | Iniciales |
| `date` | Selector de fecha |
| `datenow` | Fecha de la firma, rellenada automáticamente |
| `number` | Número |
| `select` | Desplegable |
| `multiple` | Selección múltiple |
| `radio` | Un botón de opción |
| `checkbox` | Casilla |
| `phone` | Teléfono |
| `image`, `file` | Subida de imagen o de archivo |
| `name`, `email`, `company` | Se rellenan a partir de los datos de la parte (solo OpenSign) |

Ejemplos:

```
{{Client Name;role=Client}}
{{Client Sign;type=signature;role=Client}}
{{Start Date;type=date;format=DD/MM/YYYY;role=Client}}
{{Region;type=select;options=North,South,East,West}}
{{Plan;type=radio;option=Basic}} Basic   {{Plan;type=radio;option=Plus}} Plus
{{type=checkbox;role=Client}} I accept the terms
{{Amount;type=number;format=sek;min=0}}
```

Una etiqueta que no se puede usar se deja en la página como texto, y OpenSign le explica por qué al subir el archivo: un tipo que OpenSign no tiene (como `payment`, `stamp` o `verification` de DocuSeal), un atributo sin valor, un valor que el campo no acepta o una etiqueta partida en dos líneas. Los atributos que OpenSign no necesita, como las fuentes y los colores, se ignoran.

### Escribir etiquetas en Word

Escriba las etiquetas como texto normal. Mantenga cada etiqueta en una sola línea: acorte el nombre o reduzca la fuente si Word la parte. El tamaño de fuente de la etiqueta determina la altura del campo, así que una etiqueta en texto de 8 puntos produce un campo bajo. Las etiquetas de texto funcionan en archivos Word, OpenDocument y RTF en las instalaciones con un conversor de documentos, y en cualquier PDF cuyo texto se pueda leer. Las páginas escaneadas no tienen texto, así que no tienen etiquetas.

### Desactivar las etiquetas

Las etiquetas se leen en cada subida. Para subir un archivo cuyo texto contiene por casualidad `{{` y `}}`, desactívelas mediante la API con `textTags=false`. Las API de [Scrive](scrive.md) y [DocuSign](docusign.md) nunca leen etiquetas.

## Formularios PDF

Un formulario PDF rellenable (un AcroForm) se convierte en campos al subirlo: los cuadros de texto se convierten en campos de texto, las casillas en casillas, los botones de opción en un grupo de botones de opción, las listas en desplegables o selecciones múltiples, y los cuadros de firma en campos de firma. Los valores con los que se rellenó el formulario pasan a ser valores por defecto, y los campos marcados como obligatorios en el formulario siguen siéndolo.

Un campo de formulario cuyo nombre empieza con el nombre de una parte va a esa parte: `Tenant.Name` y `Tenant name` van a Tenant. Los demás van a la primera parte que firma. El formulario en sí se quita del archivo, como en cualquier subida, de modo que solo quedan los campos de OpenSign. Los formularios XFA no se leen.

## Detectar campos

**Detectar campos** en la barra de herramientas de campos examina las páginas en busca de lugares que piden un campo:

- líneas y tiras de guiones bajos para escribir encima, con la etiqueta al lado o debajo,
- recuadros vacíos para marcar, con el texto que tienen al lado como etiqueta.

La etiqueta decide el tipo: Signature, Underskrift o Firma se convierte en un campo de firma, Name o Namnförtydligande en un nombre, Date o Datum en la fecha de la firma, Date of birth en un selector de fecha, Email, Phone y Company en sus campos, y cualquier otra cosa en texto.

Las sugerencias se dibujan con bordes discontinuos y no se guarda nada hasta que las acepta. Haga clic en una para añadirla, o en **Aceptar todos**. Van a la parte elegida en la barra de herramientas; las firmas y las iniciales van a un firmante. Pase el cursor sobre una sugerencia y haga clic en la cruz para descartarla. Los lugares que ya tienen un campo no se vuelven a sugerir.

### Con IA

Cuando hay un [proveedor de IA](ai.md) configurado y **Permitir enviar el texto de los documentos para revisiones** está activado, Detectar campos también ofrece **También con IA**. OpenSign envía entonces al modelo el texto de cada página con sus posiciones y los lugares que ya encontró, y añade las sugerencias del modelo marcadas con una chispa. No envía imágenes, como máximo 30 páginas y unos 40 000 caracteres, y la llamada cuenta para el presupuesto de IA. Cada detección con IA se escribe en el [registro de auditoría](audit-log.md).

## Mediante la API

`POST /api/v1/documents/{id}/files` y `documents.upload` leen las etiquetas de texto y los campos de formulario salvo que `textTags` o `formFields` sea `false`. El campo `fields` de la respuesta indica cuántos campos se crearon de cada origen, qué partes se añadieron y qué etiquetas no se pudieron usar.

`documents.detect_fields` devuelve sugerencias para un borrador sin guardar nada. Pase `ai: true` para consultar también al modelo de IA. Coloque los que quiera con `documents.set_fields`. Consulte [API](api.md).
