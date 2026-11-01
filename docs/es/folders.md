# Carpetas y archivo

Las carpetas agrupan documentos y plantillas, y el archivo aparta el trabajo terminado. Ambos solo cambian dónde se muestran las cosas. Nunca cambian quién puede ver un documento, cómo se conserva ni qué le ocurre.

## Carpetas

Las páginas **Documentos** y **Plantillas** tienen una columna de carpetas a la izquierda. **Todos los documentos** (o **Todas las plantillas**) enumera todo, esté en la carpeta que esté; al elegir una carpeta se enumera lo que está directamente en ella. En pantallas estrechas, las carpetas son un menú sobre la lista.

- Los documentos y las plantillas tienen carpetas separadas. Un documento no puede ir en una carpeta de plantillas ni al revés.
- Las carpetas se anidan hasta tres niveles, como *Clientes / Acme / 2026*.
- Los nombres son únicos dentro de una carpeta, sin distinguir mayúsculas y minúsculas.
- Un documento o una plantilla está como máximo en una carpeta, o en ninguna.

### Mover elementos a una carpeta

- Arrastre una fila a una carpeta de la columna de carpetas. Arrastre una fila seleccionada para mover todas las filas seleccionadas. Al soltar sobre **Todos los documentos** se sacan de su carpeta.
- O marque filas, abra **Acciones** y elija **Mover a carpeta**.
- En la página de un documento o de una plantilla, **Mover a carpeta** está en el menú **Más**.
- Un documento creado con **Nuevo documento** mientras hay una carpeta abierta va a esa carpeta.

Mover funciona en cualquier estado y queda registrado en el registro de auditoría como `document.moved`.

### Quién puede hacer qué

| | Carpetas de documentos | Carpetas de plantillas |
| --- | --- | --- |
| Ver las carpetas | Todos | Todos |
| Crear una carpeta | Cualquiera que pueda enviar documentos (`documents.create`) | `templates.manage` |
| Renombrar, mover o eliminar una carpeta | La persona que la creó, y cualquiera con `documents.manage_all` | `templates.manage` |
| Mover un elemento a una carpeta | Cualquiera que pueda cambiar ese elemento | Cualquiera que pueda cambiar esa plantilla |

Las carpetas se comparten con toda la organización, pero **una carpeta nunca concede acceso**. Las personas solo ven los documentos que ya podían ver: los suyos, los compartidos con ellas, o todos si su rol lo permite. Dos personas que abren la misma carpeta pueden ver documentos distintos en ella, y el recuento junto a una carpeta cuenta solo lo que usted puede ver. Consulte [Usuarios y roles](users-and-roles.md).

### Eliminar una carpeta

Eliminar una carpeta no elimina nada de lo que contiene. Sus documentos, plantillas y subcarpetas suben a la carpeta en la que estaba, o al nivel superior. Si el nombre de una subcarpeta ya está ocupado allí, se le añade entre paréntesis el nombre de la carpeta eliminada.

## Archivo

Archive los documentos terminados y las plantillas que ya no use, para que las listas normales sean breves.

- Los documentos pueden archivarse una vez que están **completados**, **rechazados**, **cancelados** o **caducados**. Los borradores y los documentos pendientes no.
- Las plantillas pueden archivarse en cualquier momento, salvo que aún esté en curso un envío masivo a partir de ellas. Una plantilla archivada no se ofrece en **Nuevo documento** y no puede usarse para un nuevo documento ni para un [envío masivo](bulk-send.md) hasta que se restaure.
- La pestaña **Archivado** junto a las pestañas de estado enumera el archivo, y la búsqueda y las carpetas también funcionan allí. Los documentos archivados conservan su carpeta.
- Archive varios a la vez marcando filas y eligiendo **Acciones → Archivar**, o use **Archivar** en el menú **Más** de la página de un documento. **Restaurar** los devuelve.

Archivar requiere el mismo permiso que cambiar el documento o la plantilla. Queda registrado en el registro de auditoría como `document.archived` y `document.unarchived`.

### Archivo y retención

El archivo no es una papelera de reciclaje ni un almacén aparte. Las [reglas de retención](retention.md) eliminan los documentos archivados con el mismo calendario que cualquier otro, y un documento con retención legal sigue retenido. Los documentos archivados se pueden seguir descargando y verificando.

## Desde la API

| Método | Ruta | |
| --- | --- | --- |
| GET | `/folders?scope=documents` | Enumera las carpetas: `documents` o `templates`, con `parentId` e `itemCount` |
| POST | `/folders` | Crea una. Cuerpo: `scope`, `name`, `parentId` opcional |
| PUT | `/folders/{id}` | La renombra o la mueve. Cuerpo: `name` y `parentId` (`null` para el nivel superior) |
| DELETE | `/folders/{id}` | La elimina; su contenido sube un nivel |
| POST | `/documents/move` | Cuerpo: `ids` (hasta 200) y `folderId` (`null` para ninguna carpeta) |
| POST | `/documents/archive` | Cuerpo: `ids` (hasta 200) y `archived` (`true` para archivar, `false` para restaurar) |

`GET /documents` y `GET /templates` aceptan `folderId` (solo lo que está directamente en esa carpeta) y `archived=true` (el archivo en lugar de la lista normal). `POST /documents` acepta `folderId`. `documents.counts` devuelve un recuento `archived` junto a los recuentos por estado. Consulte [API REST](api.md).
