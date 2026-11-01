# Documentación de OpenSign

OpenSign es un servicio de firma electrónica de código abierto que usted mismo aloja. Sube un documento Word o PDF, coloca los campos y lo envía. Cada firmante recibe un enlace, demuestra quién es del modo que usted haya elegido y firma. Cuando todos han terminado, recibe un PDF sellado que cualquier lector de PDF puede verificar, con páginas de evidencias que registran quién firmó, cuándo y cómo se identificó.

Esta documentación sigue su idioma, y el menú de idioma de la parte superior lo cambia. Las traducciones son automáticas y la versión en inglés es la que prevalece.

Estas páginas cubren todas las partes de OpenSign, para quienes envían documentos, para los administradores que gestionan una instalación y para los desarrolladores que se integran con ella.

## Por dónde empezar

| Usted quiere | Lea |
| --- | --- |
| Probar OpenSign o ponerlo en producción | [Inicio rápido](quick-start.md) y después [Despliegue](deployment.md) |
| Terminar de configurar una instalación nueva | [Configuración inicial y correo electrónico](setup.md) |
| Enviar su primer documento | [Documentos](documents.md) y [Partes, roles y orden de firma](parties.md) |
| Pedir a los firmantes que usen BankID, Freja o códigos SMS | [Identificación](identification.md) |
| Enviar una plantilla a cientos de personas | [Envío masivo](bulk-send.md) |
| Comprobar que un PDF firmado es auténtico | [Verificar un documento](verification.md) |
| Conectar un CRM u otro sistema | [API REST](api.md) y [Webhooks](webhooks.md) |
| Guardar PDF firmados en Google Drive, publicar en Slack, actualizar HubSpot | [Automatizaciones](automations.md) mediante Composio |
| Migrar desde Scrive o DocuSign | [Compatibilidad con Scrive](scrive.md) o [Compatibilidad con DocuSign](docusign.md) |
| Trabajar con OpenSign desde Claude | [MCP y Claude](mcp.md) |
| Entender cómo protege OpenSign los documentos | [Modelo de seguridad](security.md) |

## Qué hace OpenSign

- **Flujos de firma.** Firmantes, aprobadores y espectadores, en un orden establecido o todos a la vez. Los enlaces se envían por correo electrónico o SMS, usted mismo los entrega, o el firmante firma en persona en su dispositivo. Los recordatorios, las fechas de caducidad y el rechazo con un motivo vienen incluidos. Consulte [Partes, roles y orden de firma](parties.md).
- **Identificación.** Un enlace por correo electrónico, un código SMS, BankID sueco (con su propio acuerdo bancario o a través de TIC Identity), Freja eID u otras eID nórdicas mediante un bróker de OpenID Connect. Si la parte tiene un número de identificación personal, solo esa persona puede firmar. Consulte [Identificación](identification.md).
- **Plantillas y envío masivo.** Guarde un documento preparado como plantilla y envíelo a cada fila de un archivo CSV o Excel. Consulte [Plantillas](templates.md) y [Envío masivo](bulk-send.md).
- **Adjuntos de los firmantes.** Pida archivos a una parte, como una copia de un documento de identidad, que se limpian y se sellan dentro del PDF firmado. Consulte [Adjuntos de los firmantes](attachments.md).
- **PDF sellados.** Sellos PAdES B-LTA con un sello de tiempo RFC 3161 y todo lo necesario para validar el archivo sin conexión durante años. Consulte [PDF sellados y evidencias](sealed-pdfs.md).
- **Carpetas y archivo.** Ordene documentos y plantillas en carpetas compartidas y archive el trabajo terminado para que las listas sean breves. Consulte [Carpetas y archivo](folders.md).
- **Retención y retención legal.** Elimine documentos automáticamente tras periodos determinados y conserve los que importan con una retención legal. Consulte [Retención y retención legal](retention.md).
- **Usuarios y roles.** Administrador, gestor, remitente, auditor y espectador, con llaves de acceso o una aplicación de autenticación para el inicio de sesión en dos pasos. Consulte [Usuarios y roles](users-and-roles.md).
- **API.** Una API REST con OpenAPI, endpoints compatibles con Scrive y DocuSign, webhooks firmados y un servidor MCP para Claude y otros clientes de IA. Consulte [API REST](api.md).
- **IA, si la quiere.** Un asistente y una revisión de los documentos antes de enviarlos. Desactivada hasta que un administrador elija un proveedor. Consulte [Asistente y revisión con IA](ai.md).
- **Seguridad ante todo.** Cifrado en reposo, un registro de auditoría a prueba de manipulaciones, cabeceras de seguridad web estrictas y comprobaciones de permisos en el servidor en cada llamada. Consulte [Modelo de seguridad](security.md).

## Cómo encaja todo

OpenSign es una única imagen de contenedor. Necesita Postgres, un bucket compatible con S3 y, si quiere aceptar archivos Word, un conversor Gotenberg en una red privada. La aplicación web, las API, el servidor MCP y el worker en segundo plano se ejecutan en la misma imagen y usan los mismos servicios, de modo que un permiso o una regla se aplica igual sea cual sea la forma en que llega una solicitud.

```
           ┌──────────────── one container ───────────────────────────┐
 browser ─▶│ React app ─ RPC ──┐                                      │
 REST/API ▶│ REST, Scrive,     ├─▶ services (permissions, audit, ──▶ Postgres
 Claude  ─▶│ DocuSign, MCP ────┤   settings, encryption)          ──▶ S3 (encrypted files)
 signer  ─▶│ signing page ─────┘   worker: email, SMS, sealing,   ──▶ Email, SMS, eID, TSA, LLM
           │                       webhooks, reminders            ──▶ Gotenberg (private)
           └──────────────────────────────────────────────────────────┘
```

OpenSign tiene licencia MIT. El código fuente está en [GitHub](https://github.com/nielsbosma/OpenSign).
