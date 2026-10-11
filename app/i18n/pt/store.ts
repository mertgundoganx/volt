/** Portuguese for what the store says: toasts, confirmations, file dialogs, and the error strip's sentences. */
export default {
  // Deleting, and what goes with it
  'Delete “{name}”? The file is removed from disk.': 'Excluir “{name}”? O arquivo é removido do disco.',
  'Delete the empty folder “{name}”?': 'Excluir a pasta vazia “{name}”?',
  '1 request': '1 requisição',
  '{n} requests': '{n} requisições',
  '1 folder': '1 pasta',
  '{n} folders': '{n} pastas',
  ' and ': ' e ',
  'Delete “{name}” and the {parts} inside it? This removes them from disk.':
    'Excluir “{name}” com {parts} dentro? Tudo isso é removido do disco.',
  'Delete': 'Excluir',
  'Cancel': 'Cancelar',
  'That item is no longer in the collection.': 'Esse item não está mais na coleção.',
  'It is in the bin': 'Está na lixeira',
  'It is in the bin · 1 monitor stopped': 'Está na lixeira · 1 monitor parado',
  'It is in the bin · {n} monitors stopped': 'Está na lixeira · {n} monitores parados',
  'Deleted {name}': 'Excluído: {name}',
  '{name} is back': '{name} voltou',
  'Empty the bin? What is in it is gone for good.': 'Esvaziar a lixeira? O que está nela some de vez.',
  'Empty the bin': 'Esvaziar a lixeira',
  'Empty it': 'Esvaziar',

  // Collections and files
  'No collection is open. Use “Open…” or “New” first.': 'Nenhuma coleção está aberta. Use “Abrir…” ou “Novo” primeiro.',
  'This file could not be read and is not in the tree: {files}': 'Este arquivo não pôde ser lido e não está na árvore: {files}',
  'These files could not be read and are not in the tree: {files}': 'Estes arquivos não puderam ser lidos e não estão na árvore: {files}',
  'Moved to {where}': 'Movida para {where}',
  '{id} there; the copy here is in the bin': '{id} lá; a cópia daqui está na lixeira',
  'Copied to {where}': 'Copiada para {where}',
  'as {id}': 'como {id}',
  "Choose an export: Postman, Insomnia, OpenAPI, HAR, a Bruno collection's bruno.json, or a Postman environment":
    'Escolha uma exportação: Postman, Insomnia, OpenAPI, HAR, o bruno.json de uma coleção do Bruno ou um ambiente do Postman',
  'Postman, Insomnia, Bruno, OpenAPI, HAR or environment': 'Postman, Insomnia, Bruno, OpenAPI, HAR ou ambiente',
  'Choose where to create the collection': 'Escolha onde criar a coleção',
  'Environment {name} imported': 'Ambiente {name} importado',
  'Secret values went to the gitignored .env file.': 'Os valores secretos foram para o arquivo .env, ignorado pelo git.',
  'Export as an OpenAPI document': 'Exportar como documento OpenAPI',
  'Exported as OpenAPI': 'Exportada como OpenAPI',
  'Export as Postman collection': 'Exportar como coleção do Postman',
  'Exported for Postman': 'Exportada para o Postman',
  'Write the documentation': 'Gerar a documentação',
  'Documentation written': 'Documentação gerada',

  // Tabs and requests
  'this request': 'esta requisição',
  'Close “{name}”? Its unsaved changes are lost.': 'Fechar “{name}”? As alterações não salvas serão perdidas.',
  'Close': 'Fechar',
  'Keep open': 'Manter aberta',
  'Open a request to fill it from curl.': 'Abra uma requisição para preenchê-la a partir do curl.',
  'Open a request to copy it as curl.': 'Abra uma requisição para copiá-la como curl.',
  'Secrets kept as {names}': 'Segredos mantidos como {names}',
  'No value for {names}': 'Sem valor para {names}',
  'Copied as cURL, with secret values': 'Copiada como cURL, com os valores secretos',
  'Copied as cURL': 'Copiada como cURL',
  'Nothing to save.': 'Nada para salvar.',
  'Open a request first.': 'Abra uma requisição primeiro.',
  'Folder name cannot be empty.': 'O nome da pasta não pode ficar vazio.',
  'Name cannot be empty.': 'O nome não pode ficar vazio.',
  'That row is no longer where it was; try again.': 'Essa linha não está mais onde estava; tente de novo.',

  // Captures, examples, cookies
  'Already captured': 'Já capturado',
  '{from} is on the Captures tab': '{from} está na aba Capturas',
  'Capture added: {name}': 'Captura adicionada: {name}',
  '{from} — send again to fill it': '{from} — envie de novo para preenchê-la',
  '{from} — send again to fill it, kept as a secret': '{from} — envie de novo para preenchê-la; guardada como segredo',
  'Captured {name}': 'Capturado: {name}',
  'Captured {n} values': 'Valores capturados: {n}',
  'Saved in {name}': 'Salvo em {name}',
  'Send a request first, then keep its response.': 'Envie uma requisição primeiro e depois guarde a resposta.',
  'Example saved': 'Exemplo salvo',
  'Kept as “{name}” beside the request': 'Guardado como “{name}” junto à requisição',
  'Cookies cleared': 'Cookies limpos',
  'The next request starts with an empty jar.': 'A próxima requisição começa sem nenhum cookie.',

  // Updates
  'volt {version} is available': 'O volt {version} está disponível',
  'Install it from Settings': 'Instale pelas Configurações',
  'volt is up to date': 'O volt está atualizado',
  'You are on {version}': 'Você está na versão {version}',
  'Could not check for updates: {error}': 'Não foi possível verificar atualizações: {error}',

  // Environments and workspaces
  'Delete “{name}”? Its environment file goes, and with it any secret values saved for it.':
    'Excluir “{name}”? O arquivo do ambiente é removido e, com ele, os valores secretos salvos para esse ambiente.',
  'Delete environment': 'Excluir ambiente',
  'A workspace needs a name.': 'Um espaço de trabalho precisa de um nome.',
  'There is already a workspace called “{name}”.': 'Já existe um espaço de trabalho chamado “{name}”.',
  'Remove the workspace “{name}”? The collections in it are left exactly where they are.':
    'Remover o espaço de trabalho “{name}”? As coleções dele ficam exatamente onde estão.',
  'Remove workspace': 'Remover espaço de trabalho',
  'Remove': 'Remover',
  'Templates written': 'Modelos gerados',
  'Nothing to template': 'Nada para gerar modelos',
  'No environment has a secret in it yet': 'Nenhum ambiente tem segredos ainda',

  // Monitors, mock, sync
  'Test failed: {from} {detail}': 'Teste falhou: {from} {detail}',
  '{op} {expected}, got {actual}': 'esperado {op} {expected}, recebido {actual}',
  'nothing': 'nada',
  '{name} failed': '{name} falhou',
  'Answered {status}': 'Respondeu {status}',
  'Mock is serving': 'Servidor mock em execução',
  'Pulled': 'Pull concluído',
  'Committed and pushed': 'Commit e push concluídos',
  'Committed': 'Commit concluído',

  // Connections and history
  'Another request is connected. Disconnect it first.': 'Outra requisição está conectada. Desconecte-a primeiro.',
  'Nothing is connected.': 'Nada está conectado.',
  'That green mark is a test': 'Essa marca verde é um teste',
  'status is 200, on the Tests tab. A failing test is what makes a run fail — here and from volt-run in CI.':
    'status is 200, na aba Testes. Um teste que falha é o que faz uma execução falhar — aqui e no volt-run do CI.',
  'This request failed when it was sent: {error}': 'Esta requisição falhou ao ser enviada: {error}',
  'Nothing from history is open.': 'Nada do histórico está aberto.',
  'Clear the request history for this collection? This cannot be undone.':
    'Limpar o histórico de requisições desta coleção? Isso não pode ser desfeito.',
  'Clear history': 'Limpar histórico',
  'Clear': 'Limpar',

  // The error strip: what went wrong, in the user's words
  'Cancelled.': 'Cancelada.',
  '{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    '{host} enviou um certificado em que o volt não confia — autoassinado, expirado ou emitido para outro nome. Se o host é seu, envie sem verificar o certificado.',
  'The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    'O servidor enviou um certificado em que o volt não confia — autoassinado, expirado ou emitido para outro nome. Se o host é seu, envie sem verificar o certificado.',
  'Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.':
    'Não foi possível conectar a {host}: não há nada escutando lá. Verifique a URL e se o servidor está em execução.',
  'Could not connect to the server: nothing is listening there. Check the URL and that the server is running.':
    'Não foi possível conectar ao servidor: não há nada escutando lá. Verifique a URL e se o servidor está em execução.',
  'Could not find {host}. Check the URL for a typo, and that you are online.':
    'Não foi possível encontrar {host}. Verifique se há erro de digitação na URL e se você está online.',
  'Could not find the server. Check the URL for a typo, and that you are online.':
    'Não foi possível encontrar o servidor. Verifique se há erro de digitação na URL e se você está online.',
  'No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sem resposta de {host} em {seconds} s. O servidor pode estar lento ou inacessível; o tempo limite fica nas Configurações e, por requisição, na aba Configurações dela.',
  'No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sem resposta do servidor em {seconds} s. O servidor pode estar lento ou inacessível; o tempo limite fica nas Configurações e, por requisição, na aba Configurações dela.',
  'No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sem resposta de {host}. O servidor pode estar lento ou inacessível; o tempo limite fica nas Configurações e, por requisição, na aba Configurações dela.',
  'No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sem resposta do servidor. O servidor pode estar lento ou inacessível; o tempo limite fica nas Configurações e, por requisição, na aba Configurações dela.',
  '{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    '{host} fechou a conexão antes de responder. Tente de novo; se continuar acontecendo, o servidor está rejeitando a requisição logo no início.',
  'The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    'O servidor fechou a conexão antes de responder. Tente de novo; se continuar acontecendo, o servidor está rejeitando a requisição logo no início.',
  'No route to {host}. Check your network connection or proxy.':
    'Não há rota até {host}. Verifique sua conexão de rede ou o proxy.',
  'No route to the server. Check your network connection or proxy.':
    'Não há rota até o servidor. Verifique sua conexão de rede ou o proxy.',
  'The proxy refused or could not reach {host}. Check the proxy in Settings.':
    'O proxy recusou a requisição ou não conseguiu alcançar {host}. Verifique o proxy nas Configurações.',
  'The proxy refused or could not reach the server. Check the proxy in Settings.':
    'O proxy recusou a requisição ou não conseguiu alcançar o servidor. Verifique o proxy nas Configurações.',
  'That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.':
    'Essa não é uma URL para a qual o volt consiga enviar. Ela precisa de um esquema e de um host, como https://api.example.com/path.',

  // The JSON path filter
  'Cannot read the path from “{rest}”': 'Não é possível ler o caminho a partir de “{rest}”',
} satisfies Record<string, string>
