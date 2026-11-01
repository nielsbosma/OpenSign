# Envío masivo

El envío masivo envía una [plantilla](templates.md) a cada fila de una hoja de cálculo. Cada fila se convierte en su propio documento, con sus propias partes y valores de campo, enviado y seguido por separado: un contrato de alquiler nuevo para cada inquilino, una actualización de política para cada empleado.

## Antes de empezar

- Prepare una plantilla con las partes, los campos, el mensaje y la identificación que quiera.
- Prepare un archivo CSV o Excel (`.xlsx`). Guarde primero como `.xlsx` los archivos `.xls` antiguos y los OpenDocument `.ods`. La primera fila da nombre a las columnas. Hasta 1000 filas y 5 MB.
- Necesita permiso para enviar documentos.

## Enviar

1. Abra la plantilla y elija **Envío masivo** en su menú.
2. Elija el archivo. OpenSign lo lee y muestra cada columna con su primer valor y lo que **Rellena**.
3. Compruebe la asignación. Cambie el destino de cualquier columna o elija **Ignorar**.
4. Opcionalmente, defina **Títulos de los documentos**, un patrón con nombres de columna entre llaves, como `Lease {Address}`. Si está vacío, se usan el título de la plantilla y el nombre de la primera parte.
5. Corrija las filas que OpenSign enumera en **Corrija estas filas antes de enviar**.
6. Envíe. OpenSign crea y envía los documentos en segundo plano y abre la página de progreso del envío masivo.

## Qué puede rellenar una columna

| Destino | |
| --- | --- |
| El Nombre, Correo electrónico, Móvil, Empresa o Número de identificación personal de una parte | Un conjunto para cada parte de la plantilla. |
| Un campo | Cualquier campo de la plantilla con una etiqueta, rellenado como el valor del remitente para esa fila. Los números pueden escribirse de cualquiera de los dos modos (`1 234,50` o `1,234.50`), las fechas como YYYY-MM-DD o en el formato del campo, los campos de lista por el texto de la opción, y varias opciones separadas por comas. Un valor que incumple las comprobaciones del campo hace fallar esa fila. |
| Título | El título del documento, en lugar del patrón. |
| Su referencia | El identificador externo del documento, como un número de cliente. |
| El Idioma de una parte | El idioma de los correos y de la página de firma de ese firmante, como un código como `sv` o `pt-BR`. Consulte [La página de firma](signing-page.md#languages). |

## Correspondencia de columnas

OpenSign asocia las columnas por nombre, en inglés y en sueco. Por ejemplo, «Email», «E-post» y «Mail» corresponden todas al correo electrónico de una parte, y «Mobile», «Mobil» y «Telefon» al número de móvil. Cuando una plantilla tiene varias partes, una columna que empieza con el nombre de la parte, como «Guarantor email», va a esa parte.

Si un administrador ha configurado la IA, puede dejar que la IA asocie las columnas que las reglas integradas no pudieron. Se le envían los nombres de las columnas. También recibe algunas filas de muestra, pero solo si un administrador permite enviar el texto de los documentos al proveedor de IA. Consulte [Asistente y revisión con IA](ai.md).

## Comprobaciones de filas

Antes de enviar nada, se comprueba cada fila: cada parte necesita un nombre, una dirección de correo electrónico válida si recibe el enlace por correo electrónico, un número de móvil válido si lo recibe por SMS, etcétera. Si alguna fila fallara, no se envía nada y OpenSign enumera los problemas por número de fila de la hoja de cálculo. Corrija el archivo y vuelva a elegirlo.

## Progreso

La página del envío masivo muestra cada fila con su destinatario, su estado y un enlace al documento que creó. **Envíos masivos**, al final de la página **Plantillas**, enumera las ejecuciones recientes.

**Detener** omite las filas que aún no se han enviado. Los documentos ya enviados siguen pendientes de firma; cancélelos uno a uno si es necesario.

**Exportar Datos** descarga lo que rellenó cada destinatario, una fila por documento, como Excel, CSV o JSON. Consulte [Exportar los datos rellenados](data-export.md).

## Privacidad

Los datos de las filas se guardan cifrados mientras se ejecuta el envío masivo, y se descartan de cada fila una vez enviado su documento.

## Desde la API

El envío masivo está disponible como `POST /api/v1/bulk-sends/preview` (lee un archivo y devuelve una asignación sugerida, sin enviar nada) y `POST /api/v1/bulk-sends` (envía). Consulte [API REST](api.md#bulk-send).
