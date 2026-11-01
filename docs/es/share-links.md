# Enlaces compartidos y códigos QR

Un enlace compartido permite que cualquiera inicie por su cuenta un documento a partir de una plantilla: un formulario de afiliación en su sitio web, una renuncia de responsabilidad en el mostrador, un formulario de consentimiento impreso como código QR en un cartel. Quien abre el enlace indica su nombre y su correo electrónico, confirma el correo con un código y después firma su propia copia. No se crea nada para una dirección que no se haya confirmado.

## Activar un enlace compartido

1. Abra la plantilla. Debe poder cambiarla y enviar documentos.
2. En **Enlace público**, active **Compartir con un enlace o código QR**.
3. En **Lo rellena el visitante**, elija la parte de la plantilla en la que se convierte quien abra el enlace, como «Cliente». Debe firmar o aprobar. Todas las demás partes quedan como están en la plantilla, de modo que un cofirmante de su parte sigue firmando después de ella.
4. Copie el enlace, o elija **Código QR** y descárguelo como PNG (para pantallas y documentos) o SVG (para imprimir).

El enlace tiene un aspecto como `https://sign.example.com/s/Xq3…`. Está desactivado hasta que lo active, y al desactivarlo se detiene al instante. Los documentos creados mediante el enlace se envían desde el usuario que lo activó y aparecen en su lista de documentos, con un título formado por la plantilla y el nombre del visitante.

Los enlaces compartidos necesitan que el correo electrónico esté configurado, ya que cada visitante confirma su dirección. Consulte [Ajustes](settings.md).

## Qué ve el visitante

1. Una página de inicio con su logotipo, el título y el mensaje de la plantilla, en el idioma de la plantilla. Puede cambiar a cualquiera de los [11 idiomas de firma](signing-page.md#languages).
2. Introduce su nombre y su correo electrónico (y su número de teléfono, cuando la parte recibe códigos SMS o enlaces por SMS). Un nombre solo contiene letras, no números ni direcciones, ya que pasa al título del documento y a los correos sobre él. Los mensajes de texto de los documentos de un enlace cuentan para los límites de SMS de [Identificación](identification.md).
3. OpenSign le envía por correo electrónico un código de seis dígitos válido durante 15 minutos. Lo introduce.
4. OpenSign crea un documento a partir de la plantilla con el visitante como la parte y lo envía. La página de firma se abre de inmediato; el mismo enlace también se le envía por correo electrónico, para que pueda volver a él más tarde.

Si otras partes firman antes que él en un documento que se firma en orden, se le avisa de que recibirá un correo electrónico cuando sea su turno.

La identificación definida en la parte de la plantilla sigue aplicándose. Si la parte debe identificarse con BankID o con un código SMS, el visitante lo hace en la página de firma como cualquier firmante. Consulte [Identificación](identification.md).

## Límites

Cada enlace tiene límites que usted fija en **Enlace público**:

| Límite | Qué hace |
| --- | --- |
| **Por hora** | Como máximo este número de documentos por hora (de 1 a 1000, 10 por defecto). |
| **En total** | Como máximo este número de documentos en total. Déjelo vacío para no tener límite. |
| **Último día** | El enlace deja de funcionar después de este día. |

Además de estos, OpenSign protege por su cuenta cada enlace:

- Cada dirección recibe como máximo 3 códigos por hora, y cada enlace como máximo tres veces su límite por hora (al menos 20) en códigos, de modo que un enlace no pueda usarse para inundar la bandeja de entrada de alguien.
- Un código admite 5 intentos y se usa una sola vez.
- Cada dirección IP tiene un límite de frecuencia en la página de inicio y al pedir o introducir códigos, como en la página de firma.
- Las solicitudes sin confirmar se eliminan al cabo de un día.

## Cambiar la dirección

**Nueva dirección** da al enlace una dirección nueva. La dirección antigua y todos los códigos QR impresos a partir de ella dejan de funcionar al instante. Úsela cuando un enlace se haya compartido más allá de lo previsto.

## Con la API

- `GET /api/v1/templates/{templateId}/share-link`: el enlace, sus límites y cuántos documentos ha creado.
- `PUT /api/v1/templates/{templateId}/share-link` con `enabled`, `partyId`, `perHour`, `maxTotal` y `expiresAt`.
- `POST /api/v1/templates/{templateId}/share-link/rotate`: una dirección nueva.

Estos también son herramientas MCP (`share_links_get`, `share_links_set`, `share_links_rotate`). Consulte [API REST](api.md) y [MCP](mcp.md).

Los documentos creados mediante un enlace compartido disparan `document.created` con `"source": "share_link"` y el `shareLinkId` del enlace. Consulte [Webhooks](webhooks.md#events).

## Qué se registra

El registro de auditoría de la plantilla recoge cada cambio del enlace, cada código enviado (con la dirección de correo electrónico) y cada documento creado. El registro de auditoría del nuevo documento empieza con su creación a partir del enlace compartido.
