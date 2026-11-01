# Plantillas

Una plantilla es un documento que se prepara una vez y se envía muchas veces: un contrato de trabajo, un contrato de alquiler, un formulario de consentimiento. Tiene los mismos archivos, partes, campos, mensaje y ajustes que un documento, pero nunca se envía ella misma. Cada vez que la usa, OpenSign crea un nuevo borrador a partir de ella.

## Quién puede usar plantillas

- Cualquiera que pueda enviar documentos ve todas las plantillas y puede iniciar un documento a partir de una.
- Crear y cambiar plantillas requiere el permiso **templates.manage**, que tienen los gestores y los administradores. Consulte [Usuarios y roles](users-and-roles.md).

## Crear una plantilla

1. Vaya a **Plantillas** y elija **Nueva plantilla**. OpenSign crea una plantilla sin título y la abre.
2. Póngale un título, suba los archivos y defina el mensaje, el idioma, el orden de firma y los recordatorios como en un documento.
3. Añada las partes. En una plantilla suelen ser marcadores de posición, como «Inquilino» o «Empleado», con el rol, la entrega y la identificación que desee. Quien use la plantilla rellena los nombres y los datos de contacto reales.
4. Coloque los campos de cada parte. Las [etiquetas de texto](text-tags.md) del archivo pueden colocarlos por usted: el `role` de una etiqueta nombra la parte marcador de posición, y un rol que la plantilla aún no tiene se añade.

Una plantilla sigue siendo una plantilla; no hay un paso de «enviar».

## Usar una plantilla

- **Un documento**: elija **Nuevo documento**, seleccione la plantilla y póngale un título al documento. OpenSign copia los archivos, las partes (con sus solicitudes de adjuntos), los campos, el mensaje, el idioma, el orden de firma y los recordatorios en un nuevo borrador. Rellene los nombres y los datos de contacto reales y envíelo como de costumbre.
- **Muchos documentos**: use el [envío masivo](bulk-send.md) para enviar la plantilla a cada fila de una hoja de cálculo.
- **Dejar que la gente la inicie por su cuenta**: active un [enlace compartido](share-links.md). Quien tenga el enlace o su código QR confirma su correo electrónico y firma su propia copia.
- **Desde la API**: `POST /api/v1/documents` con `templateId`, y luego defina las partes y envíe. El endpoint `newfromtemplate` de Scrive hace lo mismo. Consulte [API REST](api.md).

Para recoger lo que la gente ha rellenado en cada documento creado a partir de una plantilla, elija **Exportar Datos** en el menú **⋯** de la plantilla. Consulte [Exportar los datos rellenados](data-export.md).

Un documento creado a partir de una plantilla es independiente de ella: cambiar la plantilla más tarde no cambia los documentos ya creados a partir de ella. Los archivos de la plantilla se comparten, no se copian, de modo que no ocupan almacenamiento adicional.

Cada documento muestra de qué plantilla y versión procede bajo su título, como «De Contrato de alquiler, versión 3».

## Versiones

OpenSign guarda un historial de cada plantilla. Una versión contiene todo lo que copia un documento: el título, el mensaje, el idioma, el orden de firma, los recordatorios, el ajuste de revisión con IA, los archivos, las partes con sus solicitudes de adjuntos y cada campo con sus opciones, validación, formato, grupo de botones de opción, indicador de solo lectura, condiciones y fórmula.

- **Versiones automáticas.** Cuando alguien usa una plantilla que ha cambiado desde su última versión, OpenSign guarda primero una nueva versión. Así, cada documento puede nombrar la versión exacta de la que se creó, sin que nadie tenga que acordarse de guardar.
- **Guardar versión.** Abra **Más → Historial de versiones** y guarde el estado actual con una nota, como «Nuevo plazo de preaviso tras la ley de alquileres de 2027». Si vuelve a guardar sin cambios, solo se actualiza la nota.
- **La cabecera** de una plantilla indica en qué versión está, o «Cambiada desde la versión 4» cuando hay cambios sin guardar. Haga clic en ella para abrir el historial.

### Historial de versiones

**Historial de versiones** enumera las versiones, las más recientes primero, con quién las guardó, la nota y cuántos documentos se crearon a partir de cada una. Elija una para ver:

- qué cambió respecto a la versión anterior: ajustes, archivos, y partes y campos añadidos, quitados o modificados (por ejemplo «Campo Dirección para Inquilino: posición y obligatorio cambiados»),
- los documentos creados a partir de ella que usted puede ver,
- **Vista previa**, que muestra las páginas de la versión con sus campos colocados tal como estaban.

Cuando la plantilla tiene cambios sin guardar, **Actual** en la parte superior muestra lo que cambió desde la última versión.

### Restaurar una versión

**Restaurar** devuelve la plantilla a como estaba en esa versión: archivos, partes, campos y ajustes. No se pierde nada. Los cambios sin guardar se guardan primero como una versión, y la propia restauración se convierte en una nueva versión marcada «Restaurada desde la versión 2». Los documentos ya creados no se tocan.

Los números de identificación personal de las versiones se guardan cifrados, como en las partes, y se muestran enmascarados.

## Reemplazar un archivo

Cuando cambia el texto del contrato pero no el diseño, use el botón de reemplazar junto al archivo en **Archivos** en lugar de quitarlo y volver a colocar los campos. Suba el nuevo archivo PDF o Word y OpenSign:

- lo coloca en el mismo lugar entre los archivos de la plantilla,
- mantiene cada campo donde estaba y desplaza los campos de los archivos posteriores si cambió el número de páginas,
- mueve a su última página los campos que estaban en páginas que el nuevo archivo no tiene y los enumera, para que pueda corregirlos.

Si el nuevo archivo tiene [etiquetas de texto](text-tags.md#text-tags), las etiquetas prevalecen: se descartan los campos del archivo antiguo y se colocan las etiquetas en su lugar, añadiendo las partes que nombran.

En una plantilla, el estado anterior al reemplazo se conserva como una versión, de modo que el archivo antiguo permanece en el historial y se puede restaurar. Reemplazar también funciona en los borradores. Desde la API, `POST /api/v1/documents/{id}/files/{fileId}/replace` con el archivo como `file` multipart. Consulte [API REST](api.md#documents).

## Duplicar una plantilla

**Más → Duplicar** crea una copia llamada «Contrato de alquiler (copia)» en la misma carpeta, con todo lo que tiene la plantilla. La copia inicia su propio historial. Desde la API, `POST /api/v1/documents` con `isTemplate: true` y el original como `templateId`.

## Carpetas y archivo

Las carpetas de plantillas mantienen ordenada una lista larga de plantillas, como *RR. HH.* y *Ventas*. Archive una plantilla que ya no use: desaparece de **Nuevo documento** y del envío masivo hasta que la restaure. Consulte [Carpetas y archivo](folders.md).

## Plantillas y retención

Las reglas de retención nunca eliminan plantillas, así que sus versiones también se conservan. Eliminar una plantilla elimina su historial y los archivos que solo usaban sus versiones. Consulte [Retención y retención legal](retention.md).
