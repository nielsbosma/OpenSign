# Configuración inicial y correo electrónico

Una instalación nueva le guía por dos pasos antes de que nadie pueda usarla: crear el primer administrador y conectar el correo electrónico. Hasta que ambos estén hechos, OpenSign devuelve a todos los administradores a la página de configuración.

## Paso 1: el primer administrador

Cuando OpenSign arranca y no tiene usuarios, muestra en su registro un **código de configuración** de un solo uso:

```
No admin yet. Open https://sign.example.com and enter the setup code …
```

Abra la instalación en un navegador y rellene:

| Campo | |
| --- | --- |
| Código de configuración | El código del registro. Demuestra que tiene acceso al servidor, para que nadie que encuentre una instalación nueva en internet pueda reclamarla. |
| Organización | El nombre de su organización. Los firmantes lo ven en los correos y en la página de firma. Puede cambiarlo después en **Ajustes → Organización**. |
| Su nombre, Correo electrónico, Contraseña | Su cuenta de administrador. |

Las instalaciones hechas con el botón **Deploy to Sliplane** se saltan el código: el instalador genera un `SETUP_CODE`, y **Abrir OpenSign** en su página final lleva a `/setup#code=…`, que lo rellena. El código va en el fragmento de la URL, que el navegador nunca envía a un servidor, y la página lo quita de la barra de direcciones. Si abre la instalación de otro modo, el código aparece en la página final del instalador y en el entorno del servicio en Sliplane. Con `SETUP_CODE` definido, el registro ya no muestra ningún código.

Una vez que existe el primer administrador, el código de configuración deja de funcionar para siempre. A todos los demás se les invita desde **Ajustes → Usuarios** (consulte [Usuarios y roles](users-and-roles.md)).

## Paso 2: correo electrónico

OpenSign envía por correo electrónico las invitaciones a firmar, los recordatorios, los avisos de que un documento se ha detenido, los documentos sellados, las invitaciones de usuario y los restablecimientos de contraseña. La configuración pide un proveedor de correo que funcione, Resend o su propio servidor SMTP, y le envía un mensaje de prueba. La configuración solo termina cuando el proveedor lo acepta, y antes de eso no se pueden enviar documentos por correo electrónico.

### Resend

[Resend](https://resend.com) es el proveedor por defecto.

1. Cree una cuenta de Resend y verifique el dominio desde el que quiere enviar en **Domains**.
2. Cree una clave de API en **API Keys**, con permiso para enviar.
3. En OpenSign, introduzca la clave de API, una **Dirección del remitente** del dominio verificado (como `sign@example.com`) y un **Nombre del remitente**.

### SMTP

Elija la pestaña **Servidor SMTP** durante la configuración, o **SMTP** como **Proveedor** en **Ajustes → Correo electrónico**, para enviar a través de su propio servidor de correo o del relé SMTP de otro proveedor.

| Campo | |
| --- | --- |
| Servidor SMTP | El nombre del host, como `smtp.example.com`. Sin `smtp://` ni puerto. |
| Puerto | Normalmente 587 para STARTTLS o 465 para TLS. |
| Seguridad | **STARTTLS** o **TLS**. |
| Nombre de usuario, Contraseña | Un nombre de usuario vacío envía sin iniciar sesión. La contraseña se guarda cifrada. |
| Dirección del remitente, Nombre del remitente | El remitente. |

TLS es siempre obligatorio y el certificado debe ser válido para el nombre del servidor, de modo que las credenciales nunca viajan sin cifrar. No hay opción de texto plano. Se rechazan los servidores en redes privadas, salvo en desarrollo.

### Cambiar el correo electrónico más adelante

Los ajustes del correo electrónico están en **Ajustes → Correo electrónico**. Cambiar el proveedor, la clave, el servidor SMTP o el remitente borra el estado «verificado», y OpenSign pide un nuevo correo de prueba antes de que los documentos puedan volver a salir. **Responder a** es opcional: si está vacío, las respuestas van a la persona que envió cada documento.

## Qué configurar a continuación

Toda la demás configuración se hace en el navegador, en **Ajustes**. Nada de ello es necesario para empezar a enviar documentos con enlaces por correo electrónico.

- **Organización**: nombre, número de registro, país, zona horaria, idioma por defecto de los firmantes, color de marca y logotipo. Consulte [Ajustes de administración](settings.md).
- **SMS**: para invitaciones por SMS y códigos de un solo uso. Consulte [Identificación](identification.md#sms-codes).
- **BankID y eID**: BankID sueco, Freja eID y otras eID nórdicas. Consulte [Identificación](identification.md).
- **Firma y sello**: el nombre del sello, el motivo y la autoridad de sellado de tiempo. Consulte [PDF sellados y evidencias](sealed-pdfs.md).
- **Seguridad**: exigir el inicio de sesión en dos pasos a todos. Consulte [Inicio de sesión, llaves de acceso y dos pasos](account-security.md).
- **IA**: opcional. Consulte [Asistente y revisión con IA](ai.md).

## Fijar ajustes con variables de entorno

Cualquier ajuste de administración puede definirse con una variable de entorno llamada `OPENSIGN_<GROUP>_<FIELD>`, por ejemplo `OPENSIGN_EMAIL_API_KEY` o `OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR=true`. Un campo fijado queda bloqueado en el navegador y muestra «Definido por una variable de entorno.». Esto resulta útil para la infraestructura como código y para mantener un secreto fuera de la base de datos por completo. Consulte [Ajustes de administración](settings.md#pinning-settings) para ver la lista completa de nombres.
