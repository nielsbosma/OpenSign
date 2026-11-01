# Documentos

Un documento en OpenSign es uno o más archivos para firmar, las partes que intervienen y los campos que rellena cada parte. Usted lo prepara como borrador, lo envía y OpenSign se encarga del resto hasta que queda firmado, rechazado, cancelado o caducado.

## El ciclo de vida de un documento

| Estado | Qué significa |
| --- | --- |
| **Borrador** | Aún lo está preparando. Los archivos, las partes y los campos pueden cambiar. |
| **Pendiente** | Enviado y a la espera de firmas. Sus archivos quedan bloqueados en un único PDF que ya no puede cambiar. |
| **Firmado** | Todos los que debían actuar han firmado o aprobado. El PDF sellado está listo. |
| **Rechazado** | Una parte lo rechazó. La firma se detuvo para todos. |
| **Cancelado** | El remitente (o un gestor) lo detuvo. |
| **Caducado** | Pasó el plazo antes de que todos hubieran firmado. |

Un documento enviado no se puede editar. Para cambiar uno que está pendiente, cancélelo y empiece un nuevo borrador.

## Crear un documento

Elija **Nuevo documento** en la página **Documentos**. Suelte uno o más archivos, o parta de una [plantilla](templates.md), y póngale un título al documento.

### Archivos

- Los archivos **PDF** se aceptan tal cual, después de la limpieza (véase más abajo).
- Los archivos **Word** (`.docx`, `.doc`), **OpenDocument** (`.odt`) y **RTF** los convierte a PDF Gotenberg. Esto requiere un conversor en la instalación; sin él, solo se aceptan PDF.
- Un documento puede tener varios archivos. Al enviarlo, se combinan por orden en un único PDF.
- El tamaño máximo de subida lo fija un administrador en **Ajustes → Seguridad** (25 MB por defecto).

Las etiquetas de texto y los campos de formulario rellenables del archivo se convierten en campos al subirlo. OpenSign le dice cuántos ha añadido y qué etiquetas no ha podido usar. Consulte [Etiquetas de texto, importación de formularios y detección de campos](text-tags.md).

Cada PDF se comprueba por su contenido, no por su nombre, y se reescribe sin JavaScript, acciones de formulario, archivos incrustados, XFA ni acciones de inicio o de URI. Si se ha quitado algo, OpenSign se lo indica. Nadie, ni siquiera los firmantes, llega a ver el contenido activo original.

### Detalles

| Ajuste | |
| --- | --- |
| Título | Se muestra en la lista de documentos, en la invitación y en la página de firma. |
| Mensaje para las partes | Se muestra en la invitación y en la página de firma. |
| Idioma para los firmantes | El idioma de los correos, los SMS y la página de firma, salvo que una parte tenga el suyo propio. Si está vacío, se usa el valor por defecto de **Ajustes → Organización**. Consulte [La página de firma](signing-page.md#languages). |
| Caduca | La fecha a partir de la cual el documento ya no se puede firmar. Entonces pasa a **Caducado**. |
| Recordar cada (días) | Envía un recordatorio a las partes que aún no han firmado, cada cierto número de días (hasta 60). 0 desactiva los recordatorios. |
| Su referencia | Un identificador externo, como un número de CRM o de expediente. Puede buscarlo en la lista de documentos y se incluye en los webhooks y en la API. |

### Partes

Añada a todos los que firman, aprueban o reciben una copia. Para cada parte elija su rol, cómo le llega su enlace y cómo debe identificarse. **Firmar en orden** en la misma tarjeta las invita paso a paso. Consulte [Partes, roles y orden de firma](parties.md) e [Identificación](identification.md).

### Campos

Elija una parte en la pestaña **Campos** y coloque los campos en las páginas. Los campos también pueden venir del propio archivo: las [etiquetas de texto](text-tags.md#text-tags) como `{{Sign;type=signature;role=Tenant}}` y los campos de un [formulario PDF rellenable](text-tags.md#pdf-forms) se convierten en campos al subir, y [Detectar campos](text-tags.md#detect-fields) en la pestaña **Campos** sugiere campos donde la página tiene líneas y recuadros por rellenar. Cada campo pertenece a una parte, que lo rellena al firmar. Seleccione un campo para cambiarlo en el panel **Campo**: quién lo rellena, su etiqueta, si es obligatorio y los ajustes de abajo. Haz clic con Mayús en los campos o arrastra por la página para seleccionar varios, y alinéalos o distribúyelos en el panel lateral. Los campos se ajustan entre sí al arrastrarlos; mantén pulsado Alt para desactivarlo.

| Campo | |
| --- | --- |
| Firma | Una firma dibujada o escrita. |
| Iniciales | Las iniciales de la parte. |
| Texto | Texto libre. Puede tener un patrón, una longitud y un valor por defecto. |
| Número | Un número, con un valor mínimo y máximo y un [formato](#number-and-date-formats). |
| Desplegable | Una opción de una lista de opciones. |
| Selección múltiple | Cualquier número de opciones de una lista, escritas separadas por comas en la página. |
| Botón de opción | Un grupo de botones del que la parte elige uno. Consulte [Grupos de botones de opción](#radio-groups). |
| Casilla | Puede ser obligatoria o estar marcada por defecto. |
| Teléfono | Un número de teléfono con su prefijo de país, guardado como +46701234567. |
| Selector de fecha | Una fecha que la parte elige en un calendario, con una fecha más temprana y una más tardía y un [formato](#number-and-date-formats). |
| Imagen | La parte sube una imagen o hace una foto. Se dibuja en el recuadro, ajustada a su tamaño. |
| Archivo | La parte sube un PDF, JPEG o PNG directamente en la página. Su nombre va en el recuadro y el archivo se sella dentro del PDF firmado, como los [adjuntos](attachments.md). |
| Fórmula | Un número calculado a partir de otros campos, como un total con IVA. Consulte [Fórmulas](#formulas). |
| Fecha | Se rellena con la fecha de la firma, en el formato que elija. |
| Nombre, Correo electrónico, Empresa | Se rellenan a partir de los datos de la parte. |

Elija **Yo, ahora (prerrellenado)** en la pestaña **Campos** para colocar un valor que rellena usted mismo antes de enviar, como un número de contrato, un importe o una fecha. Los campos obligatorios de este tipo deben rellenarse antes de poder enviar el documento.

Los campos de texto, número, lista, botón de opción, casilla, teléfono y fecha de una parte pueden tener un **Valor por defecto** del que partir. Active **Solo lectura** y la parte lo ve pero no puede cambiarlo.

#### Comprobaciones y textos de error

Las comprobaciones se ejecutan en la página de firma mientras la parte escribe, y de nuevo en el servidor cuando firma, de modo que un valor que incumpla una comprobación nunca puede llegar al PDF sellado.

| Comprobación | Se aplica a | |
| --- | --- | --- |
| Formato permitido | Texto, iniciales, empresa | Un valor predefinido (dirección de correo electrónico, solo dígitos, código postal sueco, número de identificación personal o número de organización) o una expresión regular propia. El valor completo debe coincidir. |
| Longitud mínima, Longitud máxima | Texto, iniciales, empresa | En caracteres. |
| Mínimo, Máximo | Número | Inclusivo. |
| Más temprana, Más tardía | Selector de fecha | Inclusivo. |
| Obligatorio | Todos los campos que rellena la parte | |

**Texto de error** sustituye el error integrado por uno propio, para cualquier comprobación de ese campo, como *Escriba el número de pedido como AB1234*. Sin él, la parte ve un error integrado en su propio idioma.

Los patrones usan la sintaxis habitual de las expresiones regulares: clases como `[0-9]` y `\d`, repeticiones como `{3}`, `+` y `?`, alternativas con `|` y grupos. No se admiten las anticipaciones (look-ahead) ni las referencias inversas, lo que mantiene rápida cada comprobación.

#### Formatos de número y de fecha

El formato decide cómo se escribe un valor en la página; lo que se guarda sigue siendo simple (`1234.5`, `2026-12-31`), de modo que las exportaciones y la API siempre reciben la misma forma.

Escriba un formato de número como se vería 1234 en él. `1,234.00` da una coma entre los millares y dos decimales, `1 234,00 kr` un espacio, una coma decimal y una moneda detrás, y `$1234` ningún decimal con un signo de dólar. Los valores se redondean con la mitad hacia arriba según los decimales del formato. Las partes pueden escribir los números de cualquiera de los dos modos, como `1 234,5` o `1,234.50`.

Las fechas pueden escribirse como YYYY-MM-DD, DD/MM/YYYY, MM/DD/YYYY, DD.MM.YYYY o D MMM YYYY (31 dic 2026). Los mismos formatos se aplican a la fecha de la firma.

#### Grupos de botones de opción

Al colocar **Botón de opción** se añade un botón. Selecciónelo y elija **Añadir un botón al grupo** para cada opción adicional, y luego arrastre los botones a donde correspondan. Cada botón contiene una opción; el grupo comparte su etiqueta y si se exige una respuesta. La parte puede elegir un botón del grupo.

#### Campos condicionales

En **Mostrar Solo Cuando** del panel Campo, añada condiciones sobre otros campos. El campo se muestra solo mientras se cumplan todas, por ejemplo *Número de organización* solo cuando *Tipo* es *Empresa*, o un recuadro *Motivo* solo cuando la casilla *Otro* está marcada. Los campos con condiciones se dibujan con un borde discontinuo en el editor.

| Regla | |
| --- | --- |
| Es, No Es | Compara con una opción, un número o un texto. El texto se compara sin distinguir mayúsculas y minúsculas, y los números por su valor, de modo que 10 y 10.0 son iguales. |
| Está Rellenado, Está Vacío | Para una casilla: Está Marcada, No Está Marcada. |
| Contiene | Una lista de selección múltiple tiene la opción marcada, o un texto contiene las palabras. |
| Es Mayor Que, Es Menor Que | Números y fechas (Es Posterior a, Es Anterior a). |

Un campo oculto nunca es obligatorio, queda vacío cuando la parte firma y no se escribe en el PDF firmado. Un campo que depende de un campo oculto lo ve como vacío, de modo que la ocultación se propaga en cascada.

Las condiciones se refieren a los campos por su etiqueta, así que el campo al que se mire necesita una etiqueta, y solo un campo puede tenerla. Cambiar el nombre de un campo en el editor actualiza las condiciones y fórmulas que lo usan. El campo de una parte puede consultar sus propios campos, los valores que usted prerrellenó y los campos de las partes que firman antes cuando **Firmar en orden** está activado. El servidor vuelve a comprobar las condiciones cuando la parte firma, de modo que una parte no puede saltarse un campo cambiando la página.

#### Fórmulas

Un campo **Fórmula** calcula un número a partir de otros campos mientras la parte rellena el documento. La parte ve el resultado pero no puede cambiarlo, y el servidor lo vuelve a calcular a partir de los valores enviados cuando firma, de modo que el resultado guardado es fiable. Dele un [formato de número](#number-and-date-formats) para redondearlo y escribirlo, como `1 234,00 kr`.

Escriba las etiquetas de los otros campos entre llaves, y use `+ - * /` (o `× ÷`), paréntesis y estas funciones:

| Función | |
| --- | --- |
| `round(x, 2)` | Redondea la mitad hacia arriba a entre 0 y 6 decimales; sin un segundo valor, a un número entero. |
| `floor(x)`, `ceil(x)`, `abs(x)` | Hacia abajo, hacia arriba, o sin su signo. |
| `min(a, b, …)`, `max(a, b, …)`, `sum(a, b, …)` | |
| `if(condition, then, else)` | Las condiciones comparan con `=`, `!=`, `<`, `<=`, `>` y `>=`, y se combinan con `and(…)`, `or(…)` y `not(…)`. |
| `contains(list, "text")` | Si una lista de selección múltiple tiene la opción marcada, o si un texto contiene las palabras. |

Ejemplos:

- IVA sobre un precio: `{Price} * 0.25`
- Un total con IVA: `round({Hours} * {Rate} * 1.25, 2)`
- Un descuento sobre un importe: `if({Amount} >= 10000, {Amount} * 0.05, 0)`
- Un precio según el plan: `if({Plan} = "Pro", 99, 49) * {Seats}`

Un campo vacío cuenta como 0 y una casilla marcada como 1. Una fórmula que divide entre cero, o que usa un texto que no es un número, queda vacía. Una fórmula que rellena usted mismo solo puede usar sus propios campos prerrellenados y se calcula al guardar; la fórmula de una parte puede usar los mismos campos que una [condición](#conditional-fields).

Los campos se guardan como fracciones de la página, de modo que se mantienen en su sitio sea cual sea el tamaño de la página.

## Antes de enviar

El panel **Revisar** comprueba la configuración del documento mientras trabaja y enumera todo lo que impediría firmarlo o debilitaría el resultado:

- Errores, que bloquean el envío: no hay archivo, no hay firmante ni aprobador, una parte que recibe el enlace por correo electrónico o SMS pero no tiene dirección de correo electrónico ni teléfono, una parte que debe confirmar con un código SMS pero no tiene teléfono, un espectador con campos por rellenar, un campo obligatorio del remitente que está vacío.
- Advertencias, que conviene mirar: un firmante sin campo de firma (su firma aparece entonces solo en la página de evidencias), una parte con eID sin número de identificación personal (de modo que cualquiera con el enlace y una eID podría firmar), una parte sin correo electrónico ni teléfono (de modo que no recibirá la copia firmada).

Si un administrador ha configurado la IA, el mismo panel también puede pedir a una IA que lea el documento. Consulte [Asistente y revisión con IA](ai.md#document-review).

## Vista previa

**Vista previa**, junto a **Enviar para firmar**, muestra la página de firma tal como la verá una parte elegida, con sus campos, su idioma y sus pasos de identificación. Consulte [La página de firma](signing-page.md#previewing-as-a-party).

## Revisión con IA por los firmantes

Por defecto, los firmantes pueden dar a su propio asistente de IA una copia de solo lectura del documento para que se lo explique antes de firmar. Desactive **Permitir que los firmantes compartan con su IA** en **Revisión con IA por los firmantes** para impedirlo. Consulte [La página de firma](signing-page.md#review-with-your-ai).

## Enviar

Elija **Enviar para firmar**. OpenSign combina los archivos en un único PDF, lo bloquea y avisa a las primeras partes. Para las partes cuya entrega es **Enlace**, OpenSign muestra **Entregar los enlaces de firma** con cada enlace para copiar. Más tarde puede obtener un enlace nuevo desde la tarjeta de la parte, y **Nuevo enlace** revoca primero el anterior.

## Mientras está pendiente

Desde la página del documento puede:

- **Recordar** a todos aquellos a quienes les toca y que aún no han firmado.
- Obtener o sustituir el enlace de firma de una parte.
- Iniciar **Firmar en persona** para una parte que firma en su dispositivo. Consulte [Firma en persona](in-person.md).
- **Cancelar** el documento, con un motivo opcional. Todos los enlaces de firma dejan de funcionar y se avisa a las partes que ya habían sido notificadas.
- Ver la **Actividad**: cada paso del registro de auditoría del documento, como quién lo abrió, se identificó, firmó o descargó.

## Cuando termina

Cuando la última parte ha firmado, OpenSign sella el documento en segundo plano, normalmente en cuestión de segundos. La página del documento ofrece entonces el PDF sellado y el original tal como se envió. Las partes con dirección de correo electrónico también reciben el PDF sellado. Consulte [PDF sellados y evidencias](sealed-pdfs.md).

Cada descarga queda registrada en el registro de auditoría.

Para obtener como datos lo que rellenaron las partes, elija **Exportar Datos** en el menú **⋯**. Consulte [Exportar los datos rellenados](data-export.md).

## Compartir con compañeros

Los documentos pertenecen a la persona que los creó. Para que un compañero vea uno, compártalo con él y, si lo desea, permítale cambiarlo. Los gestores, los auditores y los administradores pueden ver todos los documentos sin que se compartan. Consulte [Usuarios y roles](users-and-roles.md).

## Eliminar

Un documento que no está pendiente se puede eliminar junto con sus archivos. Cancele primero un documento pendiente. Los documentos firmados solo puede eliminarlos quien gestiona todos los documentos, y nadie puede eliminar un documento con retención legal. La eliminación queda registrada en el registro de auditoría. Los documentos también pueden eliminarse automáticamente mediante reglas de retención. Consulte [Retención y retención legal](retention.md).

## Encontrar documentos

La página **Documentos** enumera los documentos que puede ver, los más recientes primero, con pestañas por estado. Busque por título o por su referencia.

Ordene los documentos en carpetas desde la columna de carpetas junto a la lista y archive los terminados para que salgan de las pestañas de estado. La pestaña **Archivado** los enumera. Consulte [Carpetas y archivo](folders.md).
