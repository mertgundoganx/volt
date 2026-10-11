/** Spanish for what the store says: toasts, confirmations, file dialogs, and the error strip's sentences. */
export default {
  // Deleting, and what goes with it
  'Delete “{name}”? The file is removed from disk.': '¿Eliminar “{name}”? El archivo se borra del disco.',
  'Delete the empty folder “{name}”?': '¿Eliminar la carpeta vacía “{name}”?',
  '1 request': '1 solicitud',
  '{n} requests': '{n} solicitudes',
  '1 folder': '1 carpeta',
  '{n} folders': '{n} carpetas',
  ' and ': ' y ',
  'Delete “{name}” and the {parts} inside it? This removes them from disk.':
    '¿Eliminar “{name}” junto con su contenido ({parts})? Todo se borra del disco.',
  'Delete': 'Eliminar',
  'Cancel': 'Cancelar',
  'That item is no longer in the collection.': 'Ese elemento ya no está en la colección.',
  'It is in the bin': 'Está en la papelera',
  'It is in the bin · 1 monitor stopped': 'Está en la papelera · 1 monitor detenido',
  'It is in the bin · {n} monitors stopped': 'Está en la papelera · {n} monitores detenidos',
  'Deleted {name}': 'Se eliminó {name}',
  '{name} is back': '{name} está de vuelta',
  'Empty the bin? What is in it is gone for good.': '¿Vaciar la papelera? Lo que contiene se pierde para siempre.',
  'Empty the bin': 'Vaciar la papelera',
  'Empty it': 'Vaciar',

  // Collections and files
  'No collection is open. Use “Open…” or “New” first.': 'No hay ninguna colección abierta. Usa primero “Abrir…” o “Nuevo”.',
  'This file could not be read and is not in the tree: {files}': 'Este archivo no se pudo leer y no aparece en el árbol: {files}',
  'These files could not be read and are not in the tree: {files}': 'Estos archivos no se pudieron leer y no aparecen en el árbol: {files}',
  'Moved to {where}': 'Se movió a {where}',
  '{id} there; the copy here is in the bin': 'Allí es {id}; la copia de aquí está en la papelera',
  'Copied to {where}': 'Se copió a {where}',
  'as {id}': 'como {id}',
  "Choose an export: Postman, Insomnia, OpenAPI, HAR, a Bruno collection's bruno.json, or a Postman environment":
    'Elige una exportación: Postman, Insomnia, OpenAPI, HAR, el bruno.json de una colección de Bruno o un entorno de Postman',
  'Postman, Insomnia, Bruno, OpenAPI, HAR or environment': 'Postman, Insomnia, Bruno, OpenAPI, HAR o entorno',
  'Choose where to create the collection': 'Elige dónde crear la colección',
  'Environment {name} imported': 'Se importó el entorno {name}',
  'Secret values went to the gitignored .env file.': 'Los valores secretos se guardaron en el archivo .env ignorado por git.',
  'Export as an OpenAPI document': 'Exportar como documento OpenAPI',
  'Exported as OpenAPI': 'Se exportó como OpenAPI',
  'Export as Postman collection': 'Exportar como colección de Postman',
  'Exported for Postman': 'Se exportó para Postman',
  'Write the documentation': 'Generar la documentación',
  'Documentation written': 'Documentación generada',

  // Tabs and requests
  'this request': 'esta solicitud',
  'Close “{name}”? Its unsaved changes are lost.': '¿Cerrar “{name}”? Se perderán los cambios sin guardar.',
  'Close': 'Cerrar',
  'Keep open': 'Dejar abierta',
  'Open a request to fill it from curl.': 'Abre una solicitud para rellenarla desde curl.',
  'Open a request to copy it as curl.': 'Abre una solicitud para copiarla como curl.',
  'Secrets kept as {names}': 'Los secretos quedan como {names}',
  'No value for {names}': 'Sin valor para {names}',
  'Copied as cURL, with secret values': 'Se copió como cURL, con los valores secretos',
  'Copied as cURL': 'Se copió como cURL',
  'Nothing to save.': 'No hay nada que guardar.',
  'Open a request first.': 'Abre primero una solicitud.',
  'Folder name cannot be empty.': 'El nombre de la carpeta no puede estar vacío.',
  'Name cannot be empty.': 'El nombre no puede estar vacío.',
  'That row is no longer where it was; try again.': 'Esa fila ya no está donde estaba; inténtalo de nuevo.',

  // Captures, examples, cookies
  'Already captured': 'Ya se captura',
  '{from} is on the Captures tab': '{from} está en la pestaña Capturas',
  'Capture added: {name}': 'Captura agregada: {name}',
  '{from} — send again to fill it': '{from} — envía de nuevo para darle valor',
  '{from} — send again to fill it, kept as a secret': '{from} — envía de nuevo para darle valor; se guarda como secreto',
  'Captured {name}': 'Se capturó {name}',
  'Captured {n} values': 'Valores capturados: {n}',
  'Saved in {name}': 'Se guardó en {name}',
  'Send a request first, then keep its response.': 'Envía primero una solicitud y luego conserva su respuesta.',
  'Example saved': 'Ejemplo guardado',
  'Kept as “{name}” beside the request': 'Se conservó como “{name}” junto a la solicitud',
  'Cookies cleared': 'Cookies borradas',
  'The next request starts with an empty jar.': 'La siguiente solicitud empieza sin cookies.',

  // Updates
  'volt {version} is available': 'volt {version} está disponible',
  'Install it from Settings': 'Instálalo desde Configuración',
  'volt is up to date': 'volt está actualizado',
  'You are on {version}': 'Tienes la versión {version}',
  'Could not check for updates: {error}': 'No se pudo buscar actualizaciones: {error}',

  // Environments and workspaces
  'Delete “{name}”? Its environment file goes, and with it any secret values saved for it.':
    '¿Eliminar “{name}”? Se borra su archivo de entorno y, con él, los valores secretos guardados para ese entorno.',
  'Delete environment': 'Eliminar el entorno',
  'A workspace needs a name.': 'Un espacio de trabajo necesita un nombre.',
  'There is already a workspace called “{name}”.': 'Ya existe un espacio de trabajo llamado “{name}”.',
  'Remove the workspace “{name}”? The collections in it are left exactly where they are.':
    '¿Quitar el espacio de trabajo “{name}”? Sus colecciones se quedan exactamente donde están.',
  'Remove workspace': 'Quitar el espacio de trabajo',
  'Remove': 'Quitar',
  'Templates written': 'Plantillas escritas',
  'Nothing to template': 'No hay nada que poner en una plantilla',
  'No environment has a secret in it yet': 'Aún no hay ningún entorno con secretos',

  // Monitors, mock, sync
  'Test failed: {from} {detail}': 'Prueba fallida: {from} {detail}',
  '{op} {expected}, got {actual}': '{op} {expected}, se obtuvo {actual}',
  'nothing': 'nada',
  '{name} failed': '{name} falló',
  'Answered {status}': 'Respondió {status}',
  'Mock is serving': 'El servidor simulado está en marcha',
  'Pulled': 'Pull hecho',
  'Committed and pushed': 'Commit y push hechos',
  'Committed': 'Commit hecho',

  // Connections and history
  'Another request is connected. Disconnect it first.': 'Hay otra solicitud conectada. Desconéctala primero.',
  'Nothing is connected.': 'No hay nada conectado.',
  'That green mark is a test': 'Esa marca verde es una prueba',
  'status is 200, on the Tests tab. A failing test is what makes a run fail — here and from volt-run in CI.':
    'status is 200, en la pestaña Pruebas. Una prueba fallida es lo que hace fallar una ejecución — aquí y con volt-run en CI.',
  'This request failed when it was sent: {error}': 'Esta solicitud falló cuando se envió: {error}',
  'Nothing from history is open.': 'No hay nada del historial abierto.',
  'Clear the request history for this collection? This cannot be undone.':
    '¿Borrar el historial de solicitudes de esta colección? Esta acción no se puede deshacer.',
  'Clear history': 'Borrar el historial',
  'Clear': 'Borrar',

  // The error strip: what went wrong, in the user's words
  'Cancelled.': 'Cancelado.',
  '{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    '{host} envió un certificado en el que volt no confía: autofirmado, expirado o emitido para otro nombre. Si el host está bajo tu control, envía sin verificarlo.',
  'The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    'El servidor envió un certificado en el que volt no confía: autofirmado, expirado o emitido para otro nombre. Si el host está bajo tu control, envía sin verificarlo.',
  'Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.':
    'No se pudo conectar con {host}: no hay nada escuchando ahí. Revisa la URL y que el servidor esté en marcha.',
  'Could not connect to the server: nothing is listening there. Check the URL and that the server is running.':
    'No se pudo conectar con el servidor: no hay nada escuchando ahí. Revisa la URL y que el servidor esté en marcha.',
  'Could not find {host}. Check the URL for a typo, and that you are online.':
    'No se encontró {host}. Revisa que la URL no tenga erratas y que tengas conexión.',
  'Could not find the server. Check the URL for a typo, and that you are online.':
    'No se encontró el servidor. Revisa que la URL no tenga erratas y que tengas conexión.',
  'No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sin respuesta de {host} en {seconds} s. Puede que el servidor esté lento o inaccesible; el tiempo de espera está en Configuración y, por solicitud, en su pestaña Configuración.',
  'No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sin respuesta del servidor en {seconds} s. Puede que el servidor esté lento o inaccesible; el tiempo de espera está en Configuración y, por solicitud, en su pestaña Configuración.',
  'No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sin respuesta de {host}. Puede que el servidor esté lento o inaccesible; el tiempo de espera está en Configuración y, por solicitud, en su pestaña Configuración.',
  'No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sin respuesta del servidor. Puede que el servidor esté lento o inaccesible; el tiempo de espera está en Configuración y, por solicitud, en su pestaña Configuración.',
  '{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    '{host} cerró la conexión antes de responder. Inténtalo de nuevo; si sigue ocurriendo, el servidor está rechazando la solicitud de entrada.',
  'The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    'El servidor cerró la conexión antes de responder. Inténtalo de nuevo; si sigue ocurriendo, el servidor está rechazando la solicitud de entrada.',
  'No route to {host}. Check your network connection or proxy.':
    'No hay ruta hasta {host}. Revisa tu conexión de red o el proxy.',
  'No route to the server. Check your network connection or proxy.':
    'No hay ruta hasta el servidor. Revisa tu conexión de red o el proxy.',
  'The proxy refused or could not reach {host}. Check the proxy in Settings.':
    'El proxy rechazó la solicitud o no pudo llegar a {host}. Revisa el proxy en Configuración.',
  'The proxy refused or could not reach the server. Check the proxy in Settings.':
    'El proxy rechazó la solicitud o no pudo llegar al servidor. Revisa el proxy en Configuración.',
  'That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.':
    'Esa no es una URL a la que volt pueda enviar. Necesita un esquema y un host, como https://api.example.com/path.',

  // The JSON path filter
  'Cannot read the path from “{rest}”': 'No se puede leer la ruta a partir de “{rest}”',
} satisfies Record<string, string>
