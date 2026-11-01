# Verificar un documento

Cualquiera que tenga un PDF firmado puede comprobar que es auténtico y que no ha cambiado. No se necesita cuenta.

## En la página de verificación

Abra `https://<your-install>/verify` (también enlazada como **Verificar un documento** en la barra lateral de la aplicación, e impresa en las páginas de evidencias de cada documento sellado) y suelte el PDF.

El archivo se comprueba en el servidor y no se guarda. La página responde a dos preguntas:

1. **¿Selló esta instalación exactamente este archivo?** «Sellado por este OpenSign» con el título del documento y la hora de finalización significa que el SHA-256 del archivo coincide con un documento que este servidor selló y que su sello se mantiene, hecho con uno de los certificados de sello de este servidor. «No es un documento sellado por este servidor» significa que se modificó después del sellado o que procede de otro lugar.
2. **¿Están intactas las firmas?** Cada firma y sello de tiempo del archivo se enumera con quién lo hizo, cuándo y si está intacto. Intacto solo significa que los bytes son los firmados: cualquiera puede crear un certificado con el nombre que quiera. Una firma que está intacta pero que no hizo este servidor lo indica, y el nombre que lleva no está confirmado.

## Qué cubre la comprobación

Para cada firma y sello de tiempo del documento en el archivo, OpenSign comprueba:

- que la criptografía de la firma es válida y cubre los bytes que afirma cubrir,
- si cubre el archivo completo o solo una revisión anterior,
- si todo lo añadido después son únicamente datos de validación: un almacén de seguridad del documento con certificados y respuestas de revocación, y sellos de tiempo del documento invisibles,
- la propia firma de la autoridad de sellado de tiempo (RSA de 2048 bits o más, ECDSA P-256 y P-384, SHA-2), que su certificado está destinado al sellado de tiempo y que era válido en el momento sellado. Un sello de tiempo que falla en esto no muestra ninguna hora.

Cualquier otro cambio posterior a una firma, como una anotación nueva, una edición de formulario, una entrada de catálogo modificada, un objeto eliminado o un objeto nuevo que nada parece usar, marca esa firma como no intacta. También lo hace una actualización que no está escrita del modo simple en que OpenSign escribe las suyas. Eso cierra el truco habitual de modificar un PDF firmado con una actualización incremental que algunos lectores no señalan, y significa que un archivo al que otra herramienta ha añadido datos de validación puede aparecer como modificado.

OpenSign no comprueba los firmantes ni las autoridades de sellado de tiempo contra una lista de confianza. Para eso, use un lector de PDF que tenga una.

## En un lector de PDF

Los documentos sellados son archivos PAdES estándar, así que Adobe Acrobat, Foxit y otros lectores que entienden las firmas muestran el sello, el sello de tiempo y si el documento cambió. Como el certificado del sello es autofirmado por defecto, un lector puede decir que no conoce la identidad del firmante; la página de verificación de arriba es la comprobación de referencia para su instalación. Consulte [PDF sellados y evidencias](sealed-pdfs.md#the-seal-certificate).

## Desde la API

`POST /api/v1/verify` con el PDF sin procesar como cuerpo (hasta 40 MB) devuelve el mismo resultado como JSON. No necesita clave de API y tiene un límite de frecuencia por dirección. Consulte [API REST](api.md).

```sh
curl -X POST https://sign.example.com/api/v1/verify \
  -H "Content-Type: application/pdf" --data-binary @signed.pdf
```
