# Adjuntos de los firmantes

Puede pedir a una parte que suba archivos mientras firma: una copia de un documento de identidad, un certificado, un extracto bancario. Los archivos se limpian, se guardan cifrados y se sellan dentro del PDF firmado junto con el resto de las evidencias.

## Solicitar adjuntos

En el cuadro de diálogo de la parte, en **Solicitar adjuntos**, añada una línea por cada archivo que quiera:

| Campo | |
| --- | --- |
| Nombre | Lo que pide, como «Copia del documento de identidad». Se muestra a la parte. |
| Descripción | Opcional. Más detalles para la parte. |
| Obligatorio | Un adjunto obligatorio debe subirse antes de que la parte pueda firmar. Los opcionales se pueden omitir. |

Puede pedir a cada parte hasta 10 adjuntos. Las solicitudes de adjuntos se copian cuando se crea un documento a partir de una [plantilla](templates.md) y se pueden definir mediante la API con la lista `attachments` de la parte.

## Qué ve el firmante

La página de firma enumera los archivos solicitados en **Archivos adjuntos**, cada uno con **Subir**, **Reemplazar** y **Quitar**. La parte puede cambiar lo que ha subido hasta que firma.

- Tipos aceptados: PDF, JPEG y PNG, de hasta 10 MB cada uno.
- Los archivos se comprueban por su contenido, no por su nombre.

## Cómo se limpian los archivos

- Los **PDF** se reescriben como cualquier otra subida: sin JavaScript, acciones de formulario, archivos incrustados ni XFA, y con un máximo de 50 páginas.
- Las **imágenes** se decodifican y se vuelven a codificar como PNG. Esto elimina todos los metadatos, como las posiciones GPS de las fotos de teléfono, y cualquier cosa oculta en el archivo. Las imágenes muy grandes (de más de 40 megapíxeles) se rechazan.
- Todo lo demás se rechaza.

## En el PDF sellado

Cuando se sella el documento, cada adjunto:

- se enumera en las páginas de evidencias con la parte, el nombre que usted pidió, el nombre del archivo y su hash SHA-256, y
- se incrusta en el PDF sellado como un archivo adjunto, de modo que viaja con el documento y queda cubierto por el sello.

Abra el panel de adjuntos del PDF sellado en su lector de PDF para verlos. Consulte [PDF sellados y evidencias](sealed-pdfs.md).
