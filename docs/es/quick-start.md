# Inicio rápido

Esta página le lleva desde cero hasta un documento firmado en su propia máquina. Para una instalación de producción, lea a continuación [Despliegue](deployment.md).

## 1. Ejecutar OpenSign en local

Necesita Docker y una copia del repositorio.

```sh
git clone https://github.com/nielsbosma/OpenSign.git
cd OpenSign
docker compose up -d --wait postgres s3 mailpit converter
docker compose run --rm s3-init
docker compose --profile prod up -d --build
```

Los dos primeros comandos inician Postgres, un almacén compatible con S3, Gotenberg para los archivos Word y Mailpit, y crean los buckets. El último construye e inicia la imagen de producción.

- OpenSign: http://localhost:8090
- Mailpit, una bandeja de entrada local para desarrollo: http://localhost:8026

La imagen de producción envía correo real, así que necesita una cuenta de correo real (paso 3). Para probar OpenSign sin ella, ejecútelo en modo de desarrollo, donde todos los correos llegan a Mailpit:

```sh
pnpm install
pnpm infra:up
cp .env.example .env
pnpm dev            # web app on http://localhost:5173
```

## 2. Crear el primer administrador

Una instalación nueva muestra un **código de configuración** en su registro, para que nadie más pueda reclamarla antes que usted.

```sh
docker compose logs app | grep "setup code"
```

En modo de desarrollo aparece en la terminal donde se ejecuta `pnpm dev`.

Abra OpenSign e introduzca el código de configuración, su nombre, su organización, su correo electrónico y una contraseña. Consulte [Configuración inicial y correo electrónico](setup.md) para saber qué hace cada paso.

## 3. Conectar el correo electrónico

OpenSign envía invitaciones, códigos y documentos firmados por correo electrónico, así que pide un proveedor de correo antes de que nadie pueda enviar. Introduzca una clave de API de Resend y una dirección de remitente en un dominio que haya verificado en Resend. OpenSign le envía un correo de prueba y solo continúa cuando se ha entregado.

En modo de desarrollo puede dejar la clave vacía: el correo de prueba y todo lo que sigue va a Mailpit.

## 4. Enviar un documento

1. Vaya a **Documentos** y elija **Nuevo documento**.
2. Suelte un archivo PDF o Word, póngale un título y créelo.
3. En **Partes**, añádase como firmante con su dirección de correo electrónico.
4. Elija la parte en la pestaña **Campos** y coloque un campo de **Firma** en la página.
5. Elija **Enviar para firmar**.

Abra la invitación, siga el enlace, dibuje o escriba una firma y firme. Unos segundos después el documento está **Firmado**, y puede descargar el PDF sellado desde la página del documento.

## 5. Comprobar el resultado

Abra **Verificar un documento** en la barra lateral (o vaya a `/verify`) y suelte el PDF sellado. OpenSign comprueba el sello y los sellos de tiempo y confirma que selló exactamente este archivo. Un lector de PDF que entienda las firmas, como Adobe Acrobat, también muestra el sello.

## Siguientes pasos

- [Despliegue](deployment.md): póngalo en marcha de verdad, en Sliplane o en cualquier lugar que ejecute contenedores.
- [Identificación](identification.md): configure códigos SMS, BankID o Freja.
- [Usuarios y roles](users-and-roles.md): invite a sus compañeros.
- [API REST](api.md): envíe documentos desde sus propios sistemas.
