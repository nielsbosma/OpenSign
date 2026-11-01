# Exportar los datos rellenados

Lo que los firmantes escriben en los campos acaba en el PDF sellado, y también puede obtenerlo como datos: como hoja de cálculo, como JSON, desde la API o desde un agente de IA mediante MCP.

## Desde la aplicación

Elija **Exportar Datos** y después **Excel**, **CSV** o **JSON**:

- En la página **Documentos**, junto a **Nuevo documento**. La exportación contiene los documentos que muestra la lista: la pestaña de estado, la carpeta, la búsqueda y la pestaña de archivo que haya elegido, no solo la página que ve.
- Con filas seleccionadas en la página **Documentos**, en **Acciones**, solo para esos documentos.
- En una plantilla, en el menú **⋯**, para todos los documentos creados a partir de ella.
- En un envío masivo, para todos los documentos que creó.
- En un documento, en el menú **⋯**, para ese único documento.

Una exportación contiene hasta 5000 documentos. Acote los filtros para exportar más en varias partes.

## Qué contiene

Una hoja de cálculo tiene una fila por documento, de los más antiguos a los más recientes:

| Columnas | |
| --- | --- |
| Document ID, Title, Reference, Status | El documento. Reference es su campo **Su referencia**. |
| Created, Sent, Completed | Horas en UTC. |
| Party 1 Name, Party 1 Email, Party 1 Signed | Cada parte por orden de firma, y después Party 2, etcétera. |
| Una columna por campo | Con el nombre de la etiqueta del campo. |

Los valores de los campos se escriben como los guarda OpenSign, de modo que se pueden ordenar y sumar en una hoja de cálculo:

| Campo | Valor |
| --- | --- |
| Número | Un número simple, como `1234.5`, sea cual sea el formato de visualización del campo. En Excel es un número real. |
| Fórmula | El número que calculó el servidor, como en un campo de número. |
| Selector de fecha y fecha de la firma | `YYYY-MM-DD`. |
| Casilla | `true` o `false`. En Excel, un TRUE o FALSE real. |
| Desplegable y grupo de botones de opción | La opción elegida. Un grupo de botones de opción es una columna, con el nombre de la etiqueta del grupo. |
| Selección múltiple | Las opciones elegidas, separadas por comas (una lista en JSON). |
| Teléfono | El número con su prefijo de país, como `+46701234567`. |
| Subida de imagen y de archivo | El nombre del archivo. El archivo en sí no se exporta; descárguelo desde el documento. |
| Firma | Se omite: la columna Signed de la parte indica cuándo firmó. |

Un campo que aún no se ha rellenado queda vacío, de modo que también se pueden exportar documentos pendientes. Lo mismo ocurre con un campo que una condición ocultó cuando la parte firmó.

### Nombres de columna

Una columna recibe el nombre de la etiqueta del campo, o de su tipo cuando no tiene ninguna. Cuando las partes tienen campos con la misma etiqueta, como un campo Nombre cada una, se añade la parte: `Name (Party 1)`, `Name (Party 2)`. Una etiqueta usada dos veces por la misma parte recibe un número: `Note`, `Note (2)`. Ponga etiquetas claras y únicas a los campos de la plantilla y las columnas las seguirán.

Cuando exporta documentos de plantillas distintas, cada etiqueta de campo se convierte en una columna y un documento que no tiene ese campo la deja vacía.

## Seguridad

- Solo se exportan los documentos que usted puede ver. Las personas sin acceso a todos los documentos exportan los suyos y los compartidos con ellas.
- Exportar requiere el permiso `documents.export`, que tienen los administradores, los gestores, los remitentes y los auditores. Los espectadores pueden ver los valores de un documento compartido en la página del documento, pero no pueden exportar. Consulte [Usuarios y roles](users-and-roles.md#permissions).
- Los números de identificación personal nunca se exportan, y los archivos subidos se enumeran solo por su nombre.
- A una celda de CSV que empieza con `=`, `+`, `-` o `@` se le añade un `'` inicial para que una hoja de cálculo la muestre como texto en lugar de ejecutarla como una fórmula. De lo contrario, un firmante podría escribir `=HYPERLINK(…)` en un campo y hacer que se ejecute en su ordenador. Los números negativos se dejan como están. Los archivos Excel escriben el texto como texto, así que allí no hace falta cambiar nada.
- Los archivos CSV son UTF-8 con una marca de orden de bytes, para que Excel muestre correctamente å, ü y 中.
- Cada exportación se escribe en el [registro de auditoría](audit-log.md) como `documents.exported`, con el formato, los filtros y los documentos que contenía.

## Desde la API

Los valores de un documento, con tipos:

```sh
curl https://sign.example.com/api/v1/documents/$ID/values -H "Authorization: Bearer $KEY"
```

```json
{
  "id": "019a2b11-5d0e-7a42-b6f3-1c9e0d7a4f28",
  "title": "Lease, Storgatan 1",
  "status": "completed",
  "parties": [{ "id": "…", "slot": "Party 1", "role": "signer", "name": "Anna Berg", "email": "anna@example.com", "signedAt": "2026-10-29T10:12:00Z", "…": "…" }],
  "fields": [{ "key": "Monthly rent", "label": "Monthly rent", "kind": "number", "partyId": "…", "fieldIds": ["…"], "value": 12500 }],
  "values": { "Monthly rent": 12500, "Pets": "No", "Rooms": ["Kitchen", "Bath"], "Agree to terms": true }
}
```

`values` es la vía más rápida: claves de campo asociadas a valores con tipo. `fields` tiene los mismos valores con sus ids de campo, tipos y partes. Leer los valores solo requiere acceso al documento y no queda registrado en la auditoría, como abrir el documento.

Muchos documentos a la vez:

```sh
curl -X POST https://sign.example.com/api/v1/exports -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" -d '{"format": "xlsx", "templateId": "…", "status": "completed"}'
```

El cuerpo acepta `format` (`csv`, `xlsx` o `json`) y cualquiera de `ids`, `templateId`, `bulkId`, `status`, `search`, `folderId` y `archived` (`true` solo para los documentos archivados, `false` para excluirlos, omitido para ambos). La respuesta tiene `name`, `contentType` y `count`, además de `contentBase64` para CSV y Excel o `documents` (cada uno como arriba) para JSON.

Los clientes de DocuSign obtienen los mismos valores de `GET /envelopes/{id}/form_data`; consulte [API de DocuSign](docusign.md).

Ambas llamadas también son herramientas MCP, `documents_values` y `documents_export`, de modo que un agente puede responder a «¿cuántos inquilinos dijeron que sí a las mascotas este mes?». Consulte [MCP](mcp.md).

## Con webhooks

El evento `document.completed` lleva un `valuesUrl`, la dirección `/values` de arriba, de modo que una integración puede obtener todo lo que introdujeron los firmantes en una sola llamada. Los propios webhooks nunca llevan valores de campo. Consulte [Webhooks](webhooks.md#events).
