# La página de firma

Esto es lo que ven las personas a las que envía documentos. No necesitan cuenta ni aplicación, solo un navegador. La página funciona en teléfonos, tabletas y ordenadores, en modo claro y oscuro.

## Abrir el enlace

La invitación llega por correo electrónico o SMS, o directamente de usted. Nombra a su organización y a la persona que envió el documento, e indica hasta cuándo funciona el enlace.

Si la parte debe identificarse para ver el documento, lo hace primero: con un código SMS, BankID, Freja u otra eID. Consulte [Identificación](identification.md). En caso contrario, el documento se abre de inmediato.

Un enlace que está incompleto, revocado o caducado, o que pertenece a un documento que ya no está pendiente, muestra un mensaje claro en lugar del documento.

## Leer y rellenar

La página muestra:

- de quién es el documento y el mensaje para las partes, si lo hay,
- lo que se le pide a la parte: firmar, aprobar o recibir una copia,
- el documento completo, con los campos de la propia parte resaltados.

La parte rellena sus campos: texto, números, desplegables, listas de selección múltiple, grupos de botones de opción, casillas, números de teléfono, fechas de un selector de fecha, imágenes y archivos que sube directamente, iniciales y firmas. Cada campo se comprueba mientras escribe, con errores en su idioma o el [texto de error que usted escribió](documents.md#checks-and-error-texts), y no puede firmar hasta que todos los campos pasan la comprobación. Los campos de nombre, correo electrónico y empresa se rellenan a partir de sus datos. Si pidió [adjuntos](attachments.md), los sube aquí. Si pidió una [foto de webcam](identification.md#webcam-photo), la toma aquí.

En un documento que se firma en orden, una parte que abre su enlace antes de tiempo ve «Otras personas firman antes que usted. Le enviaremos un correo cuando sea su turno».

## Firmar

Una firma puede ser:

- **Dibujar**: con un dedo, un lápiz o el ratón, o
- **Escribir**: la parte escribe su nombre y se muestra con un estilo de firma, o
- **Guardada**: una firma guardada antes, que se ofrece en primer lugar cuando hay una. Los usuarios de OpenSign que firman sus propios documentos obtienen la de su [cuenta](account-security.md#saved-signature). Los demás firmantes pueden marcar **Recordar mi firma en este navegador** cuando firman; la próxima vez que firme en ese navegador una parte con el mismo nombre, se le ofrece aquí, con **Olvidar** para quitarla. Se queda en el navegador y no se envía a ningún sitio hasta que firman con ella. No se recuerda nada durante la firma en persona en el dispositivo de otra persona.

Antes de firmar, la parte confirma: «He leído el documento y acepto firmarlo electrónicamente. Esto es tan vinculante como una firma en papel». Si la parte debe identificarse para firmar, lo hace ahora. Después elige **Firmar** (o **Aprobar** en el caso de los aprobadores).

La parte ve una confirmación. Cuando todos han firmado, las partes con dirección de correo electrónico reciben el PDF sellado por correo, y la página de firma ofrece **Descargar documento firmado** mientras el enlace siga siendo válido.

## Rechazar

**Rechazar** pide un motivo opcional y detiene el documento para todos. Se avisa al remitente, así como a todas las partes que ya tenían un enlace.

## Idiomas

Se puede dirigir a los firmantes en once idiomas: inglés, chino (simplificado), hindi, español, francés, árabe, bengalí, portugués, ruso, urdu y sueco. El árabe y el urdu se muestran de derecha a izquierda.

El idioma de cada parte determina sus correos de invitación y de recordatorio, sus mensajes SMS y su página de firma. Es el **Idioma** de la parte si lo ha definido; si no, el **Idioma para los firmantes** del documento; y si no, el valor por defecto de **Ajustes → Organización**.

La página de firma tiene un selector de idioma en su cabecera. La elección de un firmante dura solo lo que dura esa pestaña del navegador, de modo que no pasa a la persona siguiente en un dispositivo compartido.

Algunos textos siguen en inglés o en sueco: los propios mensajes de BankID, los mensajes de error del servidor y las páginas de evidencias del PDF sellado.

## Revisar con su IA

Los firmantes pueden pedir a su propio asistente de IA, como ChatGPT, Claude o Copilot, que les explique el documento antes de firmar. En **Revisar con tu IA** eligen **Crear enlace de revisión** y después copian un prompt ya preparado o eligen **Abrir en ChatGPT**, **Claude** o **Copilot**, que abre el asistente con el prompt ya rellenado.

El enlace de revisión no es el enlace de firma:

- Es de solo lectura. Da al asistente el texto del documento y el PDF tal como se envió, y no puede firmar, rechazar ni cambiar nada.
- Se muestra una sola vez y requiere la misma identificación que ver el documento.
- Caduca tras los días fijados en **Ajustes → Seguridad** (7 por defecto), o antes, en el plazo del documento o en la caducidad del enlace de firma, y admite como máximo 100 consultas.
- Deja de funcionar cuando el firmante lo revoca, cuando usted emite un nuevo enlace de firma o cuando el documento ya no está pendiente.
- Cada uso queda en el registro de auditoría.

El texto se marca para el asistente como no fiable, y las páginas con texto oculto, como texto blanco o invisible, se señalan para que el asistente pueda avisar al firmante.

Puede desactivarlo por documento en **Revisión con IA por los firmantes** en el editor del documento, lo que también revoca los enlaces ya compartidos. Un administrador puede desactivarlo para todos fijando los días en 0.

## Vista previa como una parte

Elija **Vista previa** en el editor del documento para ver la página de firma tal como la verá una parte elegida: su marca, el mensaje, su idioma, sus campos, las solicitudes de adjuntos y los pasos de identificación. Desde la vista previa no se puede firmar ni enviar nada, y no queda registrada como que la parte abrió el documento.

## Marca

La página muestra el nombre y el logotipo de su organización y usa su color de marca, todo desde **Ajustes → Organización**. Consulte [Logotipo](settings.md#logo).

La página también puede mostrarse dentro de su propia aplicación. Consulte [Firma integrada](embedding.md).

## Privacidad y seguridad

- La parte secreta del enlace va después del `#`, de modo que no se envía al servidor en las solicitudes ni en los registros, y la página la quita de la barra de direcciones una vez cargada.
- Las acciones del firmante nunca usan una sesión iniciada de OpenSign, aunque el firmante tenga una en el mismo navegador.
- Cada paso queda registrado en un registro de auditoría a prueba de manipulaciones, como la página le indica al firmante.
