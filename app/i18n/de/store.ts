/** German for what the store says: toasts, confirmations, file dialogs, and the error strip's sentences. */
export default {
  // Deleting, and what goes with it
  'Delete “{name}”? The file is removed from disk.': '„{name}“ löschen? Die Datei wird vom Datenträger entfernt.',
  'Delete the empty folder “{name}”?': 'Den leeren Ordner „{name}“ löschen?',
  '1 request': '1 Anfrage',
  '{n} requests': '{n} Anfragen',
  '1 folder': '1 Ordner',
  '{n} folders': '{n} Ordner',
  ' and ': ' und ',
  'Delete “{name}” and the {parts} inside it? This removes them from disk.':
    '„{name}“ samt Inhalt ({parts}) löschen? Alles wird vom Datenträger entfernt.',
  'Delete': 'Löschen',
  'Cancel': 'Abbrechen',
  'That item is no longer in the collection.': 'Dieses Element ist nicht mehr in der Sammlung.',
  'It is in the bin': 'Es liegt im Papierkorb',
  'It is in the bin · 1 monitor stopped': 'Es liegt im Papierkorb · 1 Monitor angehalten',
  'It is in the bin · {n} monitors stopped': 'Es liegt im Papierkorb · {n} Monitore angehalten',
  'Deleted {name}': '{name} gelöscht',
  '{name} is back': '{name} ist wieder da',
  'Empty the bin? What is in it is gone for good.': 'Papierkorb leeren? Der Inhalt ist dann endgültig weg.',
  'Empty the bin': 'Papierkorb leeren',
  'Empty it': 'Leeren',

  // Collections and files
  'No collection is open. Use “Open…” or “New” first.': 'Keine Sammlung geöffnet. Zuerst „Öffnen…“ oder „Neu“ verwenden.',
  'This file could not be read and is not in the tree: {files}': 'Diese Datei konnte nicht gelesen werden und fehlt im Baum: {files}',
  'These files could not be read and are not in the tree: {files}': 'Diese Dateien konnten nicht gelesen werden und fehlen im Baum: {files}',
  'Moved to {where}': 'Nach {where} verschoben',
  '{id} there; the copy here is in the bin': 'Dort als {id}; die Kopie hier liegt im Papierkorb',
  'Copied to {where}': 'Nach {where} kopiert',
  'as {id}': 'als {id}',
  "Choose an export: Postman, Insomnia, OpenAPI, HAR, a Bruno collection's bruno.json, or a Postman environment":
    'Einen Export wählen: Postman, Insomnia, OpenAPI, HAR, die bruno.json einer Bruno-Sammlung oder eine Postman-Umgebung',
  'Postman, Insomnia, Bruno, OpenAPI, HAR or environment': 'Postman, Insomnia, Bruno, OpenAPI, HAR oder Umgebung',
  'Choose where to create the collection': 'Speicherort für die neue Sammlung wählen',
  'Environment {name} imported': 'Umgebung {name} importiert',
  'Secret values went to the gitignored .env file.': 'Geheime Werte liegen in der von Git ignorierten .env-Datei.',
  'Export as an OpenAPI document': 'Als OpenAPI-Dokument exportieren',
  'Exported as OpenAPI': 'Als OpenAPI exportiert',
  'Export as Postman collection': 'Als Postman-Collection exportieren',
  'Exported for Postman': 'Für Postman exportiert',
  'Write the documentation': 'Dokumentation schreiben',
  'Documentation written': 'Dokumentation geschrieben',

  // Tabs and requests
  'this request': 'diese Anfrage',
  'Close “{name}”? Its unsaved changes are lost.': '„{name}“ schließen? Ungespeicherte Änderungen gehen verloren.',
  'Close': 'Schließen',
  'Keep open': 'Geöffnet lassen',
  'Open a request to fill it from curl.': 'Eine Anfrage öffnen, um sie aus curl zu füllen.',
  'Open a request to copy it as curl.': 'Eine Anfrage öffnen, um sie als curl zu kopieren.',
  'Secrets kept as {names}': 'Secrets bleiben als {names} stehen',
  'No value for {names}': 'Kein Wert für {names}',
  'Copied as cURL, with secret values': 'Als cURL kopiert, mit geheimen Werten',
  'Copied as cURL': 'Als cURL kopiert',
  'Nothing to save.': 'Nichts zu speichern.',
  'Open a request first.': 'Zuerst eine Anfrage öffnen.',
  'Folder name cannot be empty.': 'Der Ordnername darf nicht leer sein.',
  'Name cannot be empty.': 'Der Name darf nicht leer sein.',
  'That row is no longer where it was; try again.': 'Diese Zeile ist nicht mehr an ihrem Platz; bitte erneut versuchen.',

  // Captures, examples, cookies
  'Already captured': 'Wird bereits erfasst',
  '{from} is on the Captures tab': '{from} steht im Tab „Erfassungen“',
  'Capture added: {name}': 'Erfassung hinzugefügt: {name}',
  '{from} — send again to fill it': '{from} – zum Füllen erneut senden',
  '{from} — send again to fill it, kept as a secret': '{from} – zum Füllen erneut senden; wird als Secret behalten',
  'Captured {name}': '{name} erfasst',
  'Captured {n} values': '{n} Werte erfasst',
  'Saved in {name}': 'In {name} gespeichert',
  'Send a request first, then keep its response.': 'Zuerst eine Anfrage senden, dann ihre Antwort behalten.',
  'Example saved': 'Beispiel gespeichert',
  'Kept as “{name}” beside the request': 'Als „{name}“ neben der Anfrage behalten',
  'Cookies cleared': 'Cookies gelöscht',
  'The next request starts with an empty jar.': 'Die nächste Anfrage beginnt mit leerem Cookie-Speicher.',

  // Updates
  'volt {version} is available': 'volt {version} ist verfügbar',
  'Install it from Settings': 'Installation über die Einstellungen',
  'volt is up to date': 'volt ist aktuell',
  'You are on {version}': 'Installierte Version: {version}',
  'Could not check for updates: {error}': 'Suche nach Updates fehlgeschlagen: {error}',

  // Environments and workspaces
  'Delete “{name}”? Its environment file goes, and with it any secret values saved for it.':
    '„{name}“ löschen? Die Umgebungsdatei wird entfernt und mit ihr alle dafür gespeicherten geheimen Werte.',
  'Delete environment': 'Umgebung löschen',
  'A workspace needs a name.': 'Ein Arbeitsbereich braucht einen Namen.',
  'There is already a workspace called “{name}”.': 'Es gibt bereits einen Arbeitsbereich namens „{name}“.',
  'Remove the workspace “{name}”? The collections in it are left exactly where they are.':
    'Arbeitsbereich „{name}“ entfernen? Die Sammlungen darin bleiben genau dort, wo sie sind.',
  'Remove workspace': 'Arbeitsbereich entfernen',
  'Remove': 'Entfernen',
  'Templates written': 'Vorlagen geschrieben',
  'Nothing to template': 'Nichts für eine Vorlage vorhanden',
  'No environment has a secret in it yet': 'Noch enthält keine Umgebung ein Secret',

  // Monitors, mock, sync
  'Test failed: {from} {detail}': 'Test fehlgeschlagen: {from} {detail}',
  '{op} {expected}, got {actual}': '{op} {expected}, erhalten: {actual}',
  'nothing': 'nichts',
  '{name} failed': '{name} fehlgeschlagen',
  'Answered {status}': 'Antwort: {status}',
  'Mock is serving': 'Mock-Server läuft',
  'Pulled': 'Pull ausgeführt',
  'Committed and pushed': 'Committet und gepusht',
  'Committed': 'Committet',

  // Connections and history
  'Another request is connected. Disconnect it first.': 'Eine andere Anfrage ist verbunden. Diese zuerst trennen.',
  'Nothing is connected.': 'Nichts ist verbunden.',
  'That green mark is a test': 'Die grüne Markierung ist ein Test',
  'status is 200, on the Tests tab. A failing test is what makes a run fail — here and from volt-run in CI.':
    'status is 200, im Tab „Tests“. Ein fehlschlagender Test lässt einen Durchlauf scheitern – hier ebenso wie mit volt-run in CI.',
  'This request failed when it was sent: {error}': 'Diese Anfrage ist beim Senden fehlgeschlagen: {error}',
  'Nothing from history is open.': 'Nichts aus dem Verlauf ist geöffnet.',
  'Clear the request history for this collection? This cannot be undone.':
    'Anfrageverlauf dieser Sammlung leeren? Das lässt sich nicht rückgängig machen.',
  'Clear history': 'Verlauf leeren',
  'Clear': 'Leeren',

  // The error strip: what went wrong, in the user's words
  'Cancelled.': 'Abgebrochen.',
  '{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    '{host} hat ein Zertifikat gesendet, dem volt nicht vertraut – selbstsigniert, abgelaufen oder für einen anderen Namen ausgestellt. Bei einem Host unter eigener Kontrolle: ohne Prüfung senden.',
  'The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    'Der Server hat ein Zertifikat gesendet, dem volt nicht vertraut – selbstsigniert, abgelaufen oder für einen anderen Namen ausgestellt. Bei einem Host unter eigener Kontrolle: ohne Prüfung senden.',
  'Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.':
    'Keine Verbindung zu {host}: Dort lauscht nichts. URL prüfen und sicherstellen, dass der Server läuft.',
  'Could not connect to the server: nothing is listening there. Check the URL and that the server is running.':
    'Keine Verbindung zum Server: Dort lauscht nichts. URL prüfen und sicherstellen, dass der Server läuft.',
  'Could not find {host}. Check the URL for a typo, and that you are online.':
    '{host} wurde nicht gefunden. URL auf Tippfehler prüfen und sicherstellen, dass eine Internetverbindung besteht.',
  'Could not find the server. Check the URL for a typo, and that you are online.':
    'Der Server wurde nicht gefunden. URL auf Tippfehler prüfen und sicherstellen, dass eine Internetverbindung besteht.',
  'No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Keine Antwort von {host} innerhalb von {seconds} s. Der Server ist möglicherweise langsam oder nicht erreichbar; das Timeout steht in den Einstellungen und pro Anfrage im Tab „Einstellungen“ der Anfrage.',
  'No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Keine Antwort vom Server innerhalb von {seconds} s. Der Server ist möglicherweise langsam oder nicht erreichbar; das Timeout steht in den Einstellungen und pro Anfrage im Tab „Einstellungen“ der Anfrage.',
  'No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Keine Antwort von {host}. Der Server ist möglicherweise langsam oder nicht erreichbar; das Timeout steht in den Einstellungen und pro Anfrage im Tab „Einstellungen“ der Anfrage.',
  'No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Keine Antwort vom Server. Der Server ist möglicherweise langsam oder nicht erreichbar; das Timeout steht in den Einstellungen und pro Anfrage im Tab „Einstellungen“ der Anfrage.',
  '{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    '{host} hat die Verbindung vor einer Antwort geschlossen. Erneut versuchen; passiert das wiederholt, weist der Server die Anfrage frühzeitig ab.',
  'The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    'Der Server hat die Verbindung vor einer Antwort geschlossen. Erneut versuchen; passiert das wiederholt, weist der Server die Anfrage frühzeitig ab.',
  'No route to {host}. Check your network connection or proxy.':
    'Keine Route zu {host}. Netzwerkverbindung oder Proxy prüfen.',
  'No route to the server. Check your network connection or proxy.':
    'Keine Route zum Server. Netzwerkverbindung oder Proxy prüfen.',
  'The proxy refused or could not reach {host}. Check the proxy in Settings.':
    'Der Proxy hat abgelehnt oder konnte {host} nicht erreichen. Proxy in den Einstellungen prüfen.',
  'The proxy refused or could not reach the server. Check the proxy in Settings.':
    'Der Proxy hat abgelehnt oder konnte den Server nicht erreichen. Proxy in den Einstellungen prüfen.',
  'That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.':
    'An diese URL kann volt nicht senden. Nötig sind ein Schema und ein Host, etwa https://api.example.com/path.',

  // The JSON path filter
  'Cannot read the path from “{rest}”': 'Der Pfad ist ab „{rest}“ nicht lesbar',
} satisfies Record<string, string>
