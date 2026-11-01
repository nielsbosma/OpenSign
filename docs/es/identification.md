# Identificación

Para cada parte usted elige con qué rigor debe demostrar quién es, por separado para abrir el documento (**Identificarse para ver**) y para firmarlo (**Identificarse para firmar**). Lo que use queda registrado en el registro de auditoría y en las páginas de evidencias del PDF sellado.

## Métodos

| Método | Para ver | Para firmar | Requiere |
| --- | --- | --- | --- |
| **Ninguno** | Por defecto | | Tener el enlace basta para abrir el documento. |
| **Estándar** | | Por defecto | Tener el enlace. Habitual en la entrega por correo electrónico y SMS, ya que solo el destinatario recibe el enlace. |
| **Código SMS** (`sms_pin`) | Sí | Sí | Un proveedor de SMS y un número de móvil en la parte. |
| **BankID sueco** (`se_bankid`) | Sí | Sí | BankID directamente o a través de TIC Identity. |
| **Freja eID** (`freja`) | Sí | Sí | Un acuerdo con Freja eID. |
| **Otra eID** (`broker`) | Sí | Sí | Un bróker de eID de OpenID Connect, como Idura (Criipto) o Signicat. |

Un método que no está configurado en la instalación se muestra desactivado en el cuadro de diálogo de la parte. Se rechaza el envío si una parte usa un método que no está listo, de modo que un firmante nunca se queda bloqueado.

## Foto de webcam

Marque **Pedir identificación por webcam** en un firmante o aprobador. Antes de poder firmar, se hace una foto en directo con la cámara de su ordenador o con su teléfono: **Activar la cámara**, **Tomar foto** y después **Usar foto** o **Repetir**. No hay selector de archivos.

- A la foto se le quitan los datos EXIF y de ubicación, se guarda cifrada y se incrusta en el PDF sellado con su hash SHA-256. El registro de auditoría recoge cuándo se tomó.
- En la página del documento, la tarjeta de la parte muestra la foto y cuándo se tomó. Cada visualización queda registrada en el registro de auditoría.
- Solo la página de firma puede usar la cámara. Todas las demás páginas de OpenSign la bloquean.

Una foto es una evidencia de quién firmó, no una prueba de que sea en directo: un firmante hábil podría enviar una imagen fija en lugar de una en directo. Para una identificación sólida, use una eID.

## Números de identificación personal

Para BankID y Freja, indique el **Número de identificación personal** en la parte. OpenSign exige entonces que la eID nombre exactamente a esa persona: la identificación de cualquier otra se rechaza y queda registrada en el registro de auditoría. Sin un número de identificación personal, cualquiera que tenga el enlace y una eID podría firmar, y la comprobación de configuración lo advierte.

El número de identificación personal se guarda cifrado y se muestra enmascarado (solo las últimas cuatro cifras), también en el PDF sellado que recibe cada parte. El número completo que devolvió la eID permanece cifrado en la base de datos. Al editar una parte, deje el campo vacío para conservar el número guardado.

## Identificarse para ver y para firmar

**Identificarse para ver** e **Identificarse para firmar** son dos pasos distintos, incluso con la misma eID. Solo una identificación hecha para firmar le dice a la persona lo que está firmando (BankID lo muestra en la aplicación), así que una parte que tiene ambas se identifica dos veces.

## Códigos SMS

La parte pide un código en la página de firma y recibe un código de 6 dígitos por SMS.

- Un código es válido durante 10 minutos y admite 5 intentos.
- Se pueden enviar a una parte como máximo 5 códigos por hora.
- Los códigos se guardan solo como un hash con clave.
- El PDF sellado muestra el número de teléfono con solo sus tres últimas cifras.

### Configurar el SMS

En **Ajustes → SMS**, elija un proveedor:

| Proveedor | Nombre de usuario | Contraseña |
| --- | --- | --- |
| [46elks](https://46elks.com) | Nombre de usuario de la API | Contraseña de la API |
| [Twilio](https://www.twilio.com) | Account SID | Auth token |

El **Remitente** es un nombre de hasta 11 letras, o un número que usted posea. El mismo proveedor se usa para la entrega de los enlaces de firma por SMS.

### Limitar lo que puede costar el SMS

Los mensajes de texto cuestan dinero, y los enlaces compartidos permiten que personas que usted no conoce los soliciten. Estos ajustes mantienen la factura acotada:

- **Países permitidos**: los prefijos telefónicos de país que pueden recibir SMS, como `46, 47`. Si está vacío se permiten todos los países, que es como empieza una instalación para que nada de lo que ya funciona se detenga. Fíjelo en los países a los que realmente envía: algunos números en el extranjero cuestan muchas veces lo que uno local.
- **Mensajes por hora** (200 por defecto) y **Mensajes por día** (1000): para todo lo que envía la instalación. Cuando se alcanza uno, se rechazan más mensajes hasta que haya pasado el intervalo, y el servidor registra una advertencia.
- **Mensajes por hora para un enlace compartido** (30): para todos los documentos creados a partir de un enlace compartido.

0 significa sin límite.

## BankID sueco

El firmante se identifica con la aplicación BankID: en el mismo dispositivo con **Abrir BankID en este dispositivo**, o escaneando un código QR con **BankID en otro dispositivo**. Si la parte tiene un número de identificación personal, solo puede responder el BankID de esa persona.

Hay dos maneras de conectar BankID. Las partes eligen «BankID sueco» en ambos casos.

### Con su propio acuerdo de BankID

1. En **Ajustes → BankID y eID**, active **BankID sueco**.
2. Para probarlo, active **Usar el entorno de pruebas de BankID** y deje el certificado vacío. OpenSign usa el certificado de pruebas público de BankID, y puede firmar con un [BankID de prueba](https://developers.bankid.com/test-portal).
3. Para producción, desactive el entorno de pruebas y suba el certificado de parte confiable que le dio su banco (un archivo `.p12`) con su contraseña.

OpenSign habla con BankID mediante TLS mutuo y confía solo en el certificado raíz del propio BankID, que viene integrado. Sustitúyalo en **CA del servidor de BankID** solo si BankID se lo indica.

### A través de TIC Identity

[TIC Identity](https://id.tic.io) ofrece BankID sin que usted tenga un acuerdo propio con un banco.

1. En el portal de TIC, active OpenID Connect para su tenant y registre `https://<your-install>/api/eid/callback` como URI de redirección.
2. En **Ajustes → BankID y eID**, active **BankID a través de TIC Identity** e introduzca el nombre del tenant (como en `https://id.tic.io/<tenant>`), el client id y el client secret.

TIC se usa siempre que BankID con su propio certificado está desactivado. OpenSign solo confía en el número de identificación personal sueco de la reclamación `nin` de TIC, y exige que sea válido.

## Freja eID

1. En **Ajustes → BankID y eID**, active **Freja eID**. Use **Usar el entorno de pruebas de Freja** mientras prueba.
2. Suba el **Certificado de cliente** de Freja (un archivo `.pfx`) con su contraseña.
3. Añada el **Certificado de firma de resultados de Freja**. OpenSign rechaza cualquier resultado cuya firma no se compruebe con él.

El firmante confirma en la aplicación Freja.

## Otras eID nórdicas a través de un bróker

Un bróker de OpenID Connect como Idura (Criipto) o Signicat da acceso a BankID noruego y danés, MitID, identificaciones bancarias finlandesas y más.

1. Cree una aplicación en el bróker y registre `https://<your-install>/api/eid/callback` como URI de redirección.
2. En **Ajustes → BankID y eID**, active **Bróker de eID (OpenID Connect)** e introduzca la **issuer URL**, el **client id** y el **client secret**.
3. Opcionalmente, defina **Elección de eID (acr_values)** para elegir una eID, como `urn:grn:authn:no:bankid`. Si está vacío, el firmante elige en el bróker.

El firmante es enviado al bróker y de vuelta. OpenSign usa el flujo de código de autorización con PKCE, state y nonce, y comprueba la firma, el emisor, la audiencia y la caducidad del token de ID.

La identificación debe terminar en el navegador donde empezó: una cookie de corta duración une ambos, y esto vale también para TIC Identity; una respuesta que vuelve a cualquier otro lugar se rechaza y queda registrada en el registro de auditoría. Mantenga las cookies activadas para la dirección de su OpenSign.

Cuando la página de firma está integrada en otro sitio, no se puede establecer una cookie desde dentro del marco. El firmante recibe entonces una ventana en la dirección de su OpenSign que indica para qué documento es la identificación, y pasa al bróker cuando pulsa el botón. Esto solo se ofrece para partes con entrega por enlace, en una instalación que permita la integración.

## Qué se registra

Por cada identificación, OpenSign conserva el método, el nombre y el número de identificación personal (enmascarado) que devolvió la eID, la hora, la dirección IP y el navegador. Las evidencias de la eID se guardan cifradas. Aparecen en el registro de auditoría y en las páginas de evidencias del PDF sellado. Consulte [PDF sellados y evidencias](sealed-pdfs.md).
