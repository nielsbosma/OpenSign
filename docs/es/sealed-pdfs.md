# PDF sellados y evidencias

Cuando firma la última parte, OpenSign convierte el documento en un único PDF sellado. Contiene el documento con cada valor y cada firma estampados, páginas de evidencias que cuentan la historia de la firma y un sello digital que demuestra que el archivo no ha cambiado desde entonces. Cualquiera puede comprobarlo con un lector de PDF corriente o en la [página de verificación](verification.md), y sigue siendo verificable durante años, sin conexión.

## Qué contiene el PDF sellado

1. **El documento**, exactamente como se envió, con los valores de campo y las firmas de cada parte estampados en su lugar.
2. **Páginas de evidencias** al final:
   - su [logotipo](settings.md#logo), si subió uno, el título del documento, su id y su organización,
   - cada parte: nombre, correo electrónico, rol, cómo se identificó (por ejemplo «Swedish BankID, personal number 19xxxxxx-1234»), cuándo firmó y su dirección IP,
   - los eventos del registro de auditoría: enviado, abierto, identificado, firmado, etcétera, con sus horas,
   - los adjuntos que subieron los firmantes y las [fotos de webcam](identification.md#webcam-photo), con sus hashes SHA-256,
   - el hash SHA-256 del PDF original y dónde verificar el documento.
3. **Archivos incrustados**: un adjunto de evidencias en JSON con los mismos hechos en forma legible por máquina, y los [adjuntos de los firmantes](attachments.md) y las fotos de webcam.
4. **El sello**: una firma PAdES sobre todo el archivo a nombre de su organización, con un sello de tiempo RFC 3161.
5. **Datos de validación a largo plazo** y un sello de tiempo de archivo (véase más abajo).

## PAdES B-LTA

El sello sigue PAdES Baseline B-LTA, el nivel del estándar europeo para documentos que deben seguir siendo verificables durante mucho tiempo:

- **B-B**: una firma CAdES sobre todo el archivo, hecha por el sello de la organización.
- **B-T**: un sello de tiempo RFC 3161 de una autoridad de sellado de tiempo de confianza, que demuestra cuándo se hizo el sello.
- **B-LT**: todos los certificados de los que dependen el sello y el sello de tiempo, con respuestas OCSP o CRL para cada uno, guardados en el almacén de seguridad del documento del PDF. El archivo se puede validar sin contactar con nadie, incluso después de que los certificados caduquen.
- **B-LTA**: un sello de tiempo del documento sobre todo, incluidos los datos de validación.

OpenSign comprueba cada respuesta OCSP o CRL antes de guardarla: debe estar firmada por el emisor o por su respondedor delegado, referirse al certificado correcto y estar vigente. Los datos de validación y el sello de tiempo de archivo se añaden como actualizaciones incrementales, de modo que los bytes que cubre el sello nunca cambian.

Si no se pueden obtener los datos de revocación, el documento se sella igualmente y la carencia queda registrada en el registro de auditoría. Lo mismo ocurre con un certificado que un respondedor informa como revocado. Si no se puede contactar con la autoridad de sellado de tiempo tras varios intentos, el documento se sella sin sello de tiempo en lugar de quedar a la espera.

## El certificado del sello

OpenSign genera la clave del sello en el servidor la primera vez que sella un documento, como un certificado autofirmado a nombre de su organización, y la guarda cifrada. **Ajustes → Firma y sello** muestra el sujeto del certificado y cuánto tiempo es válido.

**Sustituir la clave del sello** crea un nuevo certificado autofirmado con el nombre actual de la organización. Los documentos nuevos se sellan con él. Los documentos sellados antes siguen siendo válidos y verificables.

Como el certificado es autofirmado, los lectores de PDF muestran el sello como válido y sin cambios, pero pueden decir que no conocen la identidad del firmante hasta que usted confíe en el certificado. La página de verificación comprueba que su instalación selló exactamente ese archivo.

## Ajustes

En **Ajustes → Firma y sello**:

| Ajuste | |
| --- | --- |
| Nombre del sello | Se muestra como el firmante del sello. Si está vacío, se usa el nombre de la organización. |
| Motivo de la firma | El motivo registrado en el sello. Por defecto: «Signed with OpenSign». |
| Autoridad de sellado de tiempo (RFC 3161) | La URL de la autoridad de sellado de tiempo. Por defecto: `http://timestamp.digicert.com`. |

## Obtener el PDF sellado

- **Remitentes**: descárguelo desde la página del documento, que también ofrece el original tal como se envió.
- **Partes**: todos los que tienen dirección de correo electrónico lo reciben por correo cuando el documento se completa. Los archivos de hasta 10 MB se adjuntan; los más grandes llegan como un enlace de descarga. La página de firma también ofrece **Descargar documento firmado** mientras el enlace es válido.
- **API**: `GET /api/v1/documents/{id}/file?version=signed`. Consulte [API REST](api.md).
- **Webhooks**: `document.completed` lleva el SHA-256 del archivo sellado. Consulte [Webhooks](webhooks.md).

Cada descarga queda registrada en el registro de auditoría.

## Autoridades de sellado de tiempo de confianza

Cualquiera puede ejecutar una autoridad de sellado de tiempo, así que un sello de tiempo comprobado por sí solo únicamente dice que su token es correcto. Para que la página de verificación indique en qué sellos de tiempo confía usted, enumere las huellas SHA-256 de sus certificados en **Autoridades de sellado de tiempo de confianza** en Ajustes de administración → Firma y sello. La lista está vacía por defecto.
