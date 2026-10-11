/** Italian for what the store says: toasts, confirmations, file dialogs, and the error strip's sentences. */
export default {
  // Deleting, and what goes with it
  'Delete “{name}”? The file is removed from disk.': 'Eliminare «{name}»? Il file verrà rimosso dal disco.',
  'Delete the empty folder “{name}”?': 'Eliminare la cartella vuota «{name}»?',
  '1 request': '1 richiesta',
  '{n} requests': '{n} richieste',
  '1 folder': '1 cartella',
  '{n} folders': '{n} cartelle',
  ' and ': ' e ',
  'Delete “{name}” and the {parts} inside it? This removes them from disk.':
    'Eliminare «{name}» e il suo contenuto ({parts})? Verranno rimossi dal disco.',
  'Delete': 'Elimina',
  'Cancel': 'Annulla',
  'That item is no longer in the collection.': 'Questo elemento non è più nella raccolta.',
  'It is in the bin': 'È nel cestino',
  'It is in the bin · 1 monitor stopped': 'È nel cestino · 1 monitor fermato',
  'It is in the bin · {n} monitors stopped': 'È nel cestino · {n} monitor fermati',
  'Deleted {name}': 'Eliminato: {name}',
  '{name} is back': 'Ripristinato: {name}',
  'Empty the bin? What is in it is gone for good.': 'Svuotare il cestino? Il suo contenuto andrà perso definitivamente.',
  'Empty the bin': 'Svuota il cestino',
  'Empty it': 'Svuota',

  // Collections and files
  'No collection is open. Use “Open…” or “New” first.': 'Nessuna raccolta aperta. Usa prima «Apri…» o «Nuovo».',
  'This file could not be read and is not in the tree: {files}': 'Non è stato possibile leggere questo file, che non compare nell’albero: {files}',
  'These files could not be read and are not in the tree: {files}': 'Non è stato possibile leggere questi file, che non compaiono nell’albero: {files}',
  'Moved to {where}': 'Spostata in {where}',
  '{id} there; the copy here is in the bin': 'Lì è {id}; la copia qui è nel cestino',
  'Copied to {where}': 'Copiata in {where}',
  'as {id}': 'come {id}',
  "Choose an export: Postman, Insomnia, OpenAPI, HAR, a Bruno collection's bruno.json, or a Postman environment":
    'Scegli un export: Postman, Insomnia, OpenAPI, HAR, il bruno.json di una raccolta Bruno o un ambiente Postman',
  'Postman, Insomnia, Bruno, OpenAPI, HAR or environment': 'Postman, Insomnia, Bruno, OpenAPI, HAR o ambiente',
  'Choose where to create the collection': 'Scegli dove creare la raccolta',
  'Environment {name} imported': 'Ambiente {name} importato',
  'Secret values went to the gitignored .env file.': 'I valori segreti sono finiti nel file .env ignorato da git.',
  'Export as an OpenAPI document': 'Esporta come documento OpenAPI',
  'Exported as OpenAPI': 'Esportata come OpenAPI',
  'Export as Postman collection': 'Esporta come raccolta Postman',
  'Exported for Postman': 'Esportata per Postman',
  'Write the documentation': 'Scrivi la documentazione',
  'Documentation written': 'Documentazione scritta',

  // Tabs and requests
  'this request': 'questa richiesta',
  'Close “{name}”? Its unsaved changes are lost.': 'Chiudere «{name}»? Le modifiche non salvate andranno perse.',
  'Close': 'Chiudi',
  'Keep open': 'Lascia aperta',
  'Open a request to fill it from curl.': 'Apri una richiesta per compilarla da curl.',
  'Open a request to copy it as curl.': 'Apri una richiesta per copiarla come curl.',
  'Secrets kept as {names}': 'Segreti lasciati come {names}',
  'No value for {names}': 'Nessun valore per {names}',
  'Copied as cURL, with secret values': 'Copiata come cURL, con i valori segreti',
  'Copied as cURL': 'Copiata come cURL',
  'Nothing to save.': 'Niente da salvare.',
  'Open a request first.': 'Apri prima una richiesta.',
  'Folder name cannot be empty.': 'Il nome della cartella non può essere vuoto.',
  'Name cannot be empty.': 'Il nome non può essere vuoto.',
  'That row is no longer where it was; try again.': 'Quella riga non è più dov’era; riprova.',

  // Captures, examples, cookies
  'Already captured': 'Già catturato',
  '{from} is on the Captures tab': '{from} è nella scheda Catture',
  'Capture added: {name}': 'Cattura aggiunta: {name}',
  '{from} — send again to fill it': '{from} — invia di nuovo per valorizzarla',
  '{from} — send again to fill it, kept as a secret': '{from} — invia di nuovo per valorizzarla; viene conservata come segreto',
  'Captured {name}': 'Catturato: {name}',
  'Captured {n} values': 'Valori catturati: {n}',
  'Saved in {name}': 'Salvato in {name}',
  'Send a request first, then keep its response.': 'Invia prima una richiesta, poi conservane la risposta.',
  'Example saved': 'Esempio salvato',
  'Kept as “{name}” beside the request': 'Conservato come «{name}» accanto alla richiesta',
  'Cookies cleared': 'Cookie cancellati',
  'The next request starts with an empty jar.': 'La prossima richiesta parte senza cookie.',

  // Updates
  'volt {version} is available': 'È disponibile volt {version}',
  'Install it from Settings': 'Installalo dalle Impostazioni',
  'volt is up to date': 'volt è aggiornato',
  'You are on {version}': 'Stai usando la versione {version}',
  'Could not check for updates: {error}': 'Impossibile verificare gli aggiornamenti: {error}',

  // Environments and workspaces
  'Delete “{name}”? Its environment file goes, and with it any secret values saved for it.':
    'Eliminare «{name}»? Il file dell’ambiente verrà rimosso, insieme ai valori segreti salvati per questo ambiente.',
  'Delete environment': 'Elimina l’ambiente',
  'A workspace needs a name.': 'Un’area di lavoro deve avere un nome.',
  'There is already a workspace called “{name}”.': 'Esiste già un’area di lavoro chiamata «{name}».',
  'Remove the workspace “{name}”? The collections in it are left exactly where they are.':
    'Rimuovere l’area di lavoro «{name}»? Le raccolte che contiene restano esattamente dove sono.',
  'Remove workspace': 'Rimuovi l’area di lavoro',
  'Remove': 'Rimuovi',
  'Templates written': 'Modelli scritti',
  'Nothing to template': 'Niente da cui ricavare un modello',
  'No environment has a secret in it yet': 'Nessun ambiente contiene ancora un segreto',

  // Monitors, mock, sync
  'Test failed: {from} {detail}': 'Test fallito: {from} {detail}',
  '{op} {expected}, got {actual}': 'atteso {op} {expected}, ottenuto {actual}',
  'nothing': 'niente',
  '{name} failed': '{name}: fallito',
  'Answered {status}': 'Ha risposto {status}',
  'Mock is serving': 'Il server mock è attivo',
  'Pulled': 'Pull eseguito',
  'Committed and pushed': 'Commit e push eseguiti',
  'Committed': 'Commit eseguito',

  // Connections and history
  'Another request is connected. Disconnect it first.': 'Un’altra richiesta è connessa. Disconnettila prima.',
  'Nothing is connected.': 'Niente è connesso.',
  'That green mark is a test': 'Quel segno verde è un test',
  'status is 200, on the Tests tab. A failing test is what makes a run fail — here and from volt-run in CI.':
    'status is 200, nella scheda Test. È un test fallito a far fallire un’esecuzione — qui e con volt-run in CI.',
  'This request failed when it was sent: {error}': 'Questa richiesta è fallita al momento dell’invio: {error}',
  'Nothing from history is open.': 'Nessuna richiesta della cronologia è aperta.',
  'Clear the request history for this collection? This cannot be undone.':
    'Svuotare la cronologia delle richieste di questa raccolta? L’operazione non può essere annullata.',
  'Clear history': 'Svuota la cronologia',
  'Clear': 'Svuota',

  // The error strip: what went wrong, in the user's words
  'Cancelled.': 'Annullata.',
  '{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    '{host} ha inviato un certificato che volt non considera attendibile: autofirmato, scaduto o emesso per un altro nome. Se l’host è sotto il tuo controllo, invia senza verificarlo.',
  'The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    'Il server ha inviato un certificato che volt non considera attendibile: autofirmato, scaduto o emesso per un altro nome. Se l’host è sotto il tuo controllo, invia senza verificarlo.',
  'Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.':
    'Impossibile connettersi a {host}: lì non c’è niente in ascolto. Controlla l’URL e che il server sia in esecuzione.',
  'Could not connect to the server: nothing is listening there. Check the URL and that the server is running.':
    'Impossibile connettersi al server: lì non c’è niente in ascolto. Controlla l’URL e che il server sia in esecuzione.',
  'Could not find {host}. Check the URL for a typo, and that you are online.':
    'Impossibile trovare {host}. Controlla che nell’URL non ci siano errori di battitura e di essere online.',
  'Could not find the server. Check the URL for a typo, and that you are online.':
    'Impossibile trovare il server. Controlla che nell’URL non ci siano errori di battitura e di essere online.',
  'No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Nessuna risposta da {host} entro {seconds} s. Il server potrebbe essere lento o irraggiungibile; il timeout si imposta nelle Impostazioni e, per singola richiesta, nella sua scheda Impostazioni.',
  'No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Nessuna risposta dal server entro {seconds} s. Il server potrebbe essere lento o irraggiungibile; il timeout si imposta nelle Impostazioni e, per singola richiesta, nella sua scheda Impostazioni.',
  'No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Nessuna risposta da {host}. Il server potrebbe essere lento o irraggiungibile; il timeout si imposta nelle Impostazioni e, per singola richiesta, nella sua scheda Impostazioni.',
  'No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Nessuna risposta dal server. Il server potrebbe essere lento o irraggiungibile; il timeout si imposta nelle Impostazioni e, per singola richiesta, nella sua scheda Impostazioni.',
  '{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    '{host} ha chiuso la connessione prima di rispondere. Riprova; se continua a succedere, il server sta rifiutando subito la richiesta.',
  'The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    'Il server ha chiuso la connessione prima di rispondere. Riprova; se continua a succedere, il server sta rifiutando subito la richiesta.',
  'No route to {host}. Check your network connection or proxy.':
    'Nessun percorso di rete verso {host}. Controlla la connessione di rete o il proxy.',
  'No route to the server. Check your network connection or proxy.':
    'Nessun percorso di rete verso il server. Controlla la connessione di rete o il proxy.',
  'The proxy refused or could not reach {host}. Check the proxy in Settings.':
    'Il proxy ha rifiutato la richiesta o non è riuscito a raggiungere {host}. Controlla il proxy nelle Impostazioni.',
  'The proxy refused or could not reach the server. Check the proxy in Settings.':
    'Il proxy ha rifiutato la richiesta o non è riuscito a raggiungere il server. Controlla il proxy nelle Impostazioni.',
  'That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.':
    'Questo non è un URL a cui volt può inviare. Servono uno schema e un host, come https://api.example.com/path.',

  // The JSON path filter
  'Cannot read the path from “{rest}”': 'Impossibile leggere il percorso a partire da «{rest}»',
} satisfies Record<string, string>
