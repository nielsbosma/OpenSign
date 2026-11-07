# Informar de un error

¿Ha encontrado algo que no funciona? Infórmelo desde el propio OpenSign. Su informe se convierte en una incidencia pública en GitHub, a su nombre, para que quienes desarrollan OpenSign vean lo que usted vio.

## Cómo informar

1. Abra el menú bajo su nombre, al pie de la barra lateral, y elija **Informar de un error**.
2. Se abre un panel en la parte inferior de la página. Elija una herramienta:
   - **Comentario**: haga clic en un elemento, diga qué tiene de malo y pulse Intro. Recibe un recuadro numerado.
   - **Ocultar**: arrastre sobre cualquier dato privado para taparlo en negro.
   - **Pausar**: use la página como siempre, por ejemplo para abrir antes un menú o un cuadro de diálogo. Esc también pausa.
3. Describa qué falló y qué esperaba. El título es opcional.
4. Pulse **Revisar el informe**. Verá la captura de pantalla final y cada línea que se enviará. Cambie o quite lo que no quiera compartir, o deje fuera la captura.
5. Marque la casilla que confirma que el informe puede ser público y pulse **Continuar en GitHub**.
6. Se abre una ventana nueva en el servicio de informes de errores. Muestra el informe una vez más. Inicie sesión allí con GitHub y publíquelo. La ventana enlaza después a su nueva incidencia.

Necesita una cuenta de GitHub con al menos una semana de antigüedad. Si su navegador bloquea la ventana nueva, OpenSign muestra un enlace para que la abra usted.

## Qué se publica

La incidencia muestra:

- su título y descripción, y sus comentarios con el elemento al que se refiere cada uno,
- la captura de pantalla, si la conservó,
- la página como patrón, por ejemplo `/documents/:id`, la versión de OpenSign, su navegador y sistema, el tamaño de la ventana y el idioma de la aplicación,
- los títulos de los cuadros de diálogo abiertos, los errores recientes de la consola y las solicitudes fallidas recientes, si conservó esas líneas.

Nunca muestra la dirección de su OpenSign, la dirección real de la página, su nombre ni su correo electrónico en OpenSign, ni el contenido de un documento. A las líneas técnicas se les quitan las direcciones de correo, las direcciones web, los ids, los tokens, las claves y los números largos antes de salir de OpenSign; cada uno se sustituye por `[removed]`.

La incidencia la crea su cuenta de GitHub, con la insignia de la aplicación **OpenDeploy Bug Reports**, que solo puede abrir incidencias. El servicio usa su inicio de sesión para esa única solicitud y luego lo revoca.

## Qué se tapa en la captura de pantalla

Antes de tomar la captura, OpenSign tapa en gris:

- todo lo escrito en un campo,
- las páginas y miniaturas de los PDF,
- los nombres y correos electrónicos de partes y usuarios,
- los títulos de los documentos,
- las entradas del registro de auditoría,
- las direcciones de webhook y las direcciones IP permitidas de las claves de API.

Los recuadros negros que dibuje con **Ocultar** también quedan grabados en la imagen. Nada cambia en la propia página. Revise la captura en el paso de revisión: cualquier otro dato privado le toca ocultarlo a usted.

## Quitar una captura de pantalla

Las capturas son públicas y se conservan hasta que alguien las quita. Cada incidencia termina con **Quitar esta captura de pantalla**. Ábralo, inicie sesión con GitHub, y la imagen se elimina y la incidencia lo indica. Puede quitar sus propias capturas; los mantenedores del repositorio pueden quitar cualquiera.

GitHub guarda copias en caché de las imágenes durante un tiempo, y cualquiera puede haberla guardado, así que quitar una captura no puede recuperar todas las copias. Por eso el paso de revisión le pide comprobar que no haya nada privado en pantalla.

## Qué sale de su OpenSign

No se envía nada hasta que pulsa **Continuar en GitHub**. Entonces el servidor de su OpenSign envía el informe al servicio de informes de errores en `sliplane.opendeploy.cloud`, que lo guarda como máximo una hora hasta que usted lo publica. OpenSign no guarda credenciales de GitHub.

Cada persona puede enviar 10 informes por hora, y cada OpenSign 30 por hora.

## Para administradores

Los informes de errores están activados por defecto. En **Ajustes → Informes de errores** un administrador puede desactivarlos, lo que oculta **Informar de un error** para todos, o enviar los informes a otro repositorio, como un fork. Consulte [Ajustes de administración](settings.md#bug-reports). Cada informe enviado queda anotado en el registro de auditoría.

Una instalación sin acceso saliente a internet no puede enviar informes.
