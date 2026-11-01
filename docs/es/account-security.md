# Inicio de sesión, llaves de acceso y dos pasos

Su cuenta está protegida por una contraseña y, idealmente, un segundo factor: una aplicación de autenticación o una llave de acceso. Gestione ambos en **Ajustes → Cuenta**.

## Contraseñas

- Al menos 12 caracteres. Las frases de contraseña largas son bienvenidas.
- Las contraseñas se procesan con hash Argon2id. Nadie, ni siquiera los administradores, puede leerlas.
- Tras repetidos inicios de sesión fallidos desde una dirección, esa dirección queda bloqueada para esta cuenta durante un tiempo, que empieza en un minuto y crece hasta una hora. Un día sin fallos los olvida. Solo los fallos desde varias direcciones a la vez pueden bloquear la cuenta para todos, y una llave de acceso permite iniciar sesión incluso entonces. Cada intento queda registrado.
- **Cambiar contraseña** pide la actual y cierra sus otras sesiones.
- **¿Ha olvidado su contraseña?** en la página de inicio de sesión envía por correo electrónico un enlace de restablecimiento que funciona durante una hora. Un restablecimiento cierra todas las sesiones y revoca sus claves de API, y recibe un correo electrónico si se revocó alguna. Si su cuenta tiene una llave de acceso, sigue necesitándola para iniciar sesión después.

## Dos pasos con una aplicación de autenticación

1. En **Autenticación en dos pasos**, inicie la configuración y escanee el código QR con una aplicación de autenticación, como 1Password, Google Authenticator o Microsoft Authenticator.
2. Introduzca el código de 6 dígitos para confirmar.
3. Guarde los **códigos de recuperación**. Cada uno le permite iniciar sesión una vez si pierde el teléfono. Solo se muestran esta vez.

A partir de entonces, al iniciar sesión se pide un código después de la contraseña. Si pierde a la vez el teléfono y los códigos de recuperación, un administrador puede restablecer sus dos pasos (consulte [Usuarios y roles](users-and-roles.md)).

## Llaves de acceso

Una llave de acceso permite iniciar sesión con Face ID, Touch ID, Windows Hello, su teléfono o una llave de seguridad, sin escribir una contraseña.

1. En **Llaves de acceso**, elija **Añadir una llave de acceso**, póngale un nombre (como «MacBook») y confirme su contraseña.
2. Siga la indicación de su navegador.

Después elija **Iniciar sesión con una llave de acceso** en la página de inicio de sesión. Una llave de acceso siempre le verifica en el dispositivo (con una huella, el rostro o un PIN), así que cuenta por sí sola como dos pasos y no se pide ningún código. Puede añadir varias y quitar cualquiera de ellas.

Cuando su cuenta tiene una llave de acceso y ninguna aplicación de autenticación, la contraseña por sí sola ya no le permite iniciar sesión: después de la contraseña también se le pide la llave de acceso. Si pierde todas las llaves de acceso, un administrador puede restablecer sus dos pasos, lo que las elimina.

## Confirmar que es usted

Crear una clave de API o conectar una aplicación da un acceso duradero, así que OpenSign le pide primero su contraseña o una llave de acceso, salvo que haya iniciado sesión o confirmado en los últimos diez minutos.

## Exigir dos pasos a todos

Un administrador puede activar **Exigir autenticación en dos pasos a todos** en **Ajustes → Seguridad**. A cualquiera que no tenga una aplicación de autenticación ni una llave de acceso se le pide entonces que configure una antes de poder hacer nada más. Los clientes MCP con sesión iniciada con una cuenta así son rechazados hasta que lo hagan. Recomendamos activarlo.

## Inicio de sesión único

Si su organización usa el [inicio de sesión único](single-sign-on.md), elija su botón en la página de inicio de sesión. Su proveedor de identidad se encarga entonces de su contraseña, su segundo factor y su correo electrónico, y **Ajustes → Cuenta** no tiene para usted llaves de acceso, aplicación de autenticación ni contraseña. Las claves de API y los clientes MCP no están disponibles para estas cuentas.

## Idioma

En **Ajustes → Cuenta y seguridad**, elija el idioma en que se le muestra OpenSign: inglés, chino (simplificado), hindi, español, francés, árabe, bengalí, portugués, ruso, urdu o sueco. Antes de iniciar sesión, la aplicación sigue a su navegador, y la página de inicio de sesión tiene un selector. Esto no cambia lo que ven los firmantes; ellos reciben el idioma definido en su documento o parte. Algunos mensajes del servidor siguen en inglés.

## Firma guardada

En **Ajustes → Cuenta y seguridad → Mi firma**, dibuje o escriba su firma una vez y añada sus iniciales si lo desea. Cuando firma un documento usted mismo, la página de firma se la ofrece en una pestaña **Guardada**. Nunca se aplica por sí sola: usted sigue eligiendo firmar y puede dibujar o escribir una nueva en su lugar.

Solo se ofrece cuando el enlace de firma le pertenece: cuando [firma un documento usted mismo](in-person.md#signing-a-document-yourself) o abre un enlace enviado a su propia dirección de correo electrónico con la sesión iniciada. Alguien a quien entregue su dispositivo para una firma en persona nunca la ve. El dibujo se guarda cifrado como cualquier otro archivo, y **Quitar** lo elimina. Guardarla y quitarla queda registrado en el registro de auditoría.

Los firmantes externos no tienen cuenta, así que pueden marcar **Recordar mi firma en este navegador**. Esto la conserva solo en su navegador, y se ofrece la próxima vez a una parte con el mismo nombre; nada sobre ellos se guarda en el servidor entre documentos. Consulte [La página de firma](signing-page.md#signing).

## Sesiones

Las sesiones son cookies del navegador que los scripts no pueden leer, vinculadas a su instalación y guardadas en el servidor solo como un hash con clave. En **Ajustes → Seguridad**, un administrador define:

| Ajuste | Por defecto | |
| --- | --- | --- |
| Duración de la sesión (horas) | 12 | Cuánto dura como máximo un inicio de sesión. |
| Cerrar sesión tras inactividad (minutos) | 60 | Cuánto sobrevive una sesión sin uso. |

**Cerrar sesión** en el menú de usuario termina la sesión en este navegador.

## Aplicaciones conectadas mediante MCP

En **Aplicaciones de IA (MCP)** ve la dirección del servidor MCP de su instalación y los clientes que ha conectado, como Claude. Desconecte cualquiera de ellos en cualquier momento. Consulte [MCP y Claude](mcp.md).

## Claves de API

Las claves de API se gestionan en **Ajustes → Claves de API**. Consulte [API REST](api.md#api-keys).
