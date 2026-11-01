# Firma en persona

La firma en persona permite que una parte firme en su propio ordenador o tableta, por ejemplo en un mostrador, en una reunión o en casa de un cliente. La parte no necesita una dirección de correo electrónico ni un teléfono.

## Configuración

En el cuadro de diálogo de la parte, defina **Enviar enlace por** como **En persona (en su dispositivo)**. OpenSign no envía entonces nada a esta parte. Aun así puede pedirle que se identifique con un código SMS o con una eID; lo hace en su dispositivo.

## Firmar

1. Envíe el documento como de costumbre.
2. Cuando sea el turno de la parte, abra el documento y elija **Firmar en persona** en su tarjeta de parte.
3. OpenSign abre la página de firma con una pantalla de entrega: «Entregue el dispositivo a …». Explica que todo lo que haga la parte queda registrado en las evidencias del documento.
4. La parte confirma con **Soy …**, lee el documento, se identifica si se le pide y firma.
5. Cuando termina, la página le pide que devuelva el dispositivo a usted.

## Qué se registra

- Iniciar la firma en persona emite un enlace nuevo para la parte y retira todos los anteriores, de modo que un enlace que usted iniciara antes no se puede reutilizar.
- El registro de auditoría recoge quién inició la sesión en persona y para qué parte. Las páginas de evidencias muestran la identificación de la parte, su dirección IP y su navegador, como con cualquier otro firmante.

## Firmar un documento usted mismo

Para un documento que solo usted necesita firmar, como una declaración, una confirmación de pedido o un formulario que rellena para sus propios registros, elija **Firmar yo** en el borrador en lugar de **Enviar**.

1. Elija cómo se identifica: **Mi sesión de OpenSign** (la opción por defecto), o una eID como BankID cuando un administrador haya configurado una.
2. Elija **Firmar ahora**. Usted pasa a ser el único firmante: los firmantes y aprobadores que ya estaban en el borrador se quitan y sus campos pasan a ser suyos. Las personas que reciben una copia siguen recibiéndola.
3. OpenSign envía el documento y abre la página de firma en su dispositivo, sin pantalla de entrega. Si tiene una [firma guardada](account-security.md#saved-signature), se le ofrece.
4. Cuando firma, el documento se sella y se puede verificar como cualquier otro.

Con **Mi sesión de OpenSign**, las páginas de evidencias dicen «Identified by: Signed-in OpenSign account» con su nombre y correo electrónico, y el registro de auditoría recoge el usuario que lo inició. Si se va antes de firmar, **Firmar en persona** en su tarjeta de parte vuelve a abrir la página de firma.

## Notas

- **Firmar en persona** solo se ofrece para las partes cuya entrega es En persona, mientras el documento está pendiente y es el turno de esa parte.
- Iniciar la firma en persona, o firmar un documento usted mismo, requiere un usuario con sesión iniciada. Ninguna de las dos está disponible con una clave de API ni mediante MCP.
- La entrega `pad` de Scrive se corresponde con la firma en persona. Consulte [Compatibilidad con Scrive](scrive.md).
