# Partes, roles y orden de firma

Una parte es cualquier persona que interviene en un documento: quien firma, aprueba o solo recibe la copia firmada. Las partes se añaden en la página del documento, en **Partes**.

## Datos de la parte

| Campo | |
| --- | --- |
| Nombre | Obligatorio. Se muestra en la página de firma y en las evidencias. |
| Rol | Firma, Aprueba o Recibe una copia. Véase más abajo. |
| Correo electrónico | Necesario para la entrega por correo electrónico y para recibir la copia firmada. |
| Móvil | Con prefijo de país, como `+46 70 123 45 67`. Necesario para la entrega por SMS y los códigos SMS. |
| Empresa | Opcional. Rellena los campos Empresa. |
| Idioma | Sus correos, SMS y página de firma. **Igual que el documento** por defecto. Consulte [La página de firma](signing-page.md#languages). |
| Enviar enlace por | Cómo recibe la parte su enlace de firma. Consulte [Entrega](#delivery). |
| Identificarse para ver | Lo que debe hacer la parte antes de poder abrir el documento. Consulte [Identificación](identification.md). |
| Identificarse para firmar | Lo que debe hacer la parte antes de poder firmar. Consulte [Identificación](identification.md). |
| Pedir identificación por webcam | La parte se hace una foto antes de firmar. Consulte [Foto de webcam](identification.md#webcam-photo). |
| Número de identificación personal | Para la identificación con eID. Cuando está definido, solo puede firmar la eID de esa persona. Se guarda cifrado y se muestra enmascarado. |
| Orden de firma | Cuando el documento se firma en orden, los números más bajos van primero. Consulte [Orden de firma](#signing-order). |
| Solicitar adjuntos | Archivos que la parte debe o puede subir. Consulte [Adjuntos de los firmantes](attachments.md). |

## Roles

| Rol | Qué hace la parte |
| --- | --- |
| **Firma** (firmante) | Lee el documento, rellena sus campos y firma con una firma dibujada o escrita. |
| **Aprueba** (aprobador) | Lee el documento y lo aprueba. Sin imagen de firma, pero la aprobación consta en las evidencias como una firma. |
| **Recibe una copia** (espectador) | No actúa. Recibe el PDF sellado cuando el documento se completa. Un espectador no puede tener campos. |

El documento se completa cuando todos los firmantes y aprobadores han actuado.

## Entrega

| Opción | |
| --- | --- |
| **Correo electrónico** | La opción por defecto. OpenSign envía por correo una invitación con el enlace personal y, si están activados, recordatorios. |
| **SMS** | OpenSign envía el enlace por mensaje de texto. Requiere un proveedor de SMS en **Ajustes → SMS**. |
| **Enlace (usted lo entrega)** | OpenSign no envía nada. Cuando envía el documento, le muestra el enlace para que lo transmita, por ejemplo en su propio correo o chat, o para mostrarlo dentro de su propia aplicación con la [firma integrada](embedding.md). |
| **En persona (en su dispositivo)** | La parte firma en su ordenador o tableta, iniciándolo desde la página del documento. Consulte [Firma en persona](in-person.md). |
| **Ninguno** | OpenSign no envía nada ni muestra ningún enlace. Útil para espectadores que solo necesitan la copia firmada, que va a su dirección de correo electrónico. |

Las opciones que no están configuradas en esta instalación se muestran pero no se pueden elegir hasta que un administrador las configure.

### Enlaces de firma

Cada parte tiene un enlace personal. Solo es válido para esa parte y deja de funcionar cuando:

- el documento se completa, se rechaza, se cancela o caduca,
- se agota la vida útil del propio enlace: la fecha de caducidad del documento o, si no, el número de días fijado en **Ajustes → Seguridad** (30 por defecto), contados desde que se avisó por primera vez a la parte, o
- se emite un nuevo enlace para la parte, lo que revoca el anterior.

La parte secreta del enlace va después del `#`, de modo que nunca llega a los registros del servidor, y la página de firma la quita de la barra de direcciones. Consulte [Modelo de seguridad](security.md#signing-links).

## Orden de firma

Por defecto, todos son invitados a la vez y pueden firmar en cualquier orden. Active **Firmar en orden** en la tarjeta **Partes** para invitar a las partes paso a paso:

- Cada firmante y aprobador muestra su número de paso. Al activar el orden se parte del orden en que están enumeradas las partes.
- Use las flechas de la tarjeta de una parte para moverla antes o después.
- Para que varias partes firmen juntas en el mismo paso, déles el mismo número de **Orden de firma** en **Editar**. Los números más bajos van primero.
- Cuando todos los de un paso han actuado, OpenSign invita al siguiente paso.
- Una parte que abre su enlace antes de que sea su turno ve «Otras personas firman antes que usted» y recibe un correo electrónico cuando es su turno.

Los espectadores no forman parte del orden. Reciben el PDF sellado al final.

## Rechazar

Un firmante o un aprobador puede rechazar, con un motivo opcional. Rechazar detiene el documento para todos: pasa a **Rechazado**, todos los enlaces dejan de funcionar, se avisa al remitente y se dispara un webhook `document.declined` con el motivo.

## Cambiar las partes

Las partes se pueden cambiar mientras el documento es un borrador. Quitar una parte también quita sus campos. Una vez enviado el documento, sus partes quedan fijadas; para cambiarlas, cancele el documento y empiece uno nuevo.
