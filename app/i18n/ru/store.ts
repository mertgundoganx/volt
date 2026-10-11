/** Russian for what the store says: toasts, confirmations, file dialogs, and the error strip's sentences. */
export default {
  // Deleting, and what goes with it
  'Delete “{name}”? The file is removed from disk.': 'Удалить «{name}»? Файл будет удалён с диска.',
  'Delete the empty folder “{name}”?': 'Удалить пустую папку «{name}»?',
  '1 request': '1 запрос',
  '{n} requests': 'запросов: {n}',
  '1 folder': '1 папка',
  '{n} folders': 'папок: {n}',
  ' and ': ', ',
  'Delete “{name}” and the {parts} inside it? This removes them from disk.':
    'Удалить «{name}» вместе с содержимым ({parts})? Всё это будет удалено с диска.',
  'Delete': 'Удалить',
  'Cancel': 'Отмена',
  'That item is no longer in the collection.': 'Этого элемента уже нет в коллекции.',
  'It is in the bin': 'Перемещено в корзину',
  'It is in the bin · 1 monitor stopped': 'Перемещено в корзину · остановлен 1 монитор',
  'It is in the bin · {n} monitors stopped': 'Перемещено в корзину · остановлено мониторов: {n}',
  'Deleted {name}': 'Удалено: {name}',
  '{name} is back': 'Возвращено: {name}',
  'Empty the bin? What is in it is gone for good.': 'Очистить корзину? Её содержимое будет удалено безвозвратно.',
  'Empty the bin': 'Очистить корзину',
  'Empty it': 'Очистить',

  // Collections and files
  'No collection is open. Use “Open…” or “New” first.': 'Нет открытой коллекции. Сначала воспользуйтесь «Открыть…» или «Создать».',
  'This file could not be read and is not in the tree: {files}': 'Этот файл не удалось прочитать, его нет в дереве: {files}',
  'These files could not be read and are not in the tree: {files}': 'Эти файлы не удалось прочитать, их нет в дереве: {files}',
  'Moved to {where}': 'Перемещено в {where}',
  '{id} there; the copy here is in the bin': 'там это {id}; здешняя копия в корзине',
  'Copied to {where}': 'Скопировано в {where}',
  'as {id}': 'как {id}',
  "Choose an export: Postman, Insomnia, OpenAPI, HAR, a Bruno collection's bruno.json, or a Postman environment":
    'Выберите экспорт: Postman, Insomnia, OpenAPI, HAR, файл bruno.json коллекции Bruno или окружение Postman',
  'Postman, Insomnia, Bruno, OpenAPI, HAR or environment': 'Postman, Insomnia, Bruno, OpenAPI, HAR или окружение',
  'Choose where to create the collection': 'Выберите, где создать коллекцию',
  'Environment {name} imported': 'Окружение {name} импортировано',
  'Secret values went to the gitignored .env file.': 'Секретные значения записаны в файл .env, исключённый из git.',
  'Export as an OpenAPI document': 'Экспортировать как документ OpenAPI',
  'Exported as OpenAPI': 'Экспортировано как OpenAPI',
  'Export as Postman collection': 'Экспортировать как коллекцию Postman',
  'Exported for Postman': 'Экспортировано для Postman',
  'Write the documentation': 'Записать документацию',
  'Documentation written': 'Документация записана',

  // Tabs and requests
  'this request': 'этот запрос',
  'Close “{name}”? Its unsaved changes are lost.': 'Закрыть «{name}»? Несохранённые изменения будут потеряны.',
  'Close': 'Закрыть',
  'Keep open': 'Не закрывать',
  'Open a request to fill it from curl.': 'Откройте запрос, чтобы заполнить его из curl.',
  'Open a request to copy it as curl.': 'Откройте запрос, чтобы скопировать его как curl.',
  'Secrets kept as {names}': 'Секреты оставлены как {names}',
  'No value for {names}': 'Нет значения для {names}',
  'Copied as cURL, with secret values': 'Скопировано как cURL, с секретными значениями',
  'Copied as cURL': 'Скопировано как cURL',
  'Nothing to save.': 'Сохранять нечего.',
  'Open a request first.': 'Сначала откройте запрос.',
  'Folder name cannot be empty.': 'Имя папки не может быть пустым.',
  'Name cannot be empty.': 'Имя не может быть пустым.',
  'That row is no longer where it was; try again.': 'Эта строка уже не на прежнем месте; попробуйте ещё раз.',

  // Captures, examples, cookies
  'Already captured': 'Уже захватывается',
  '{from} is on the Captures tab': '{from} есть на вкладке «Захваты»',
  'Capture added: {name}': 'Захват добавлен: {name}',
  '{from} — send again to fill it': '{from} — отправьте ещё раз, чтобы заполнить',
  '{from} — send again to fill it, kept as a secret': '{from} — отправьте ещё раз, чтобы заполнить; хранится как секрет',
  'Captured {name}': 'Захвачено: {name}',
  'Captured {n} values': 'Захвачено значений: {n}',
  'Saved in {name}': 'Сохранено в окружении {name}',
  'Send a request first, then keep its response.': 'Сначала отправьте запрос, затем сохраните его ответ.',
  'Example saved': 'Пример сохранён',
  'Kept as “{name}” beside the request': 'Сохранён как «{name}» рядом с запросом',
  'Cookies cleared': 'Cookie очищены',
  'The next request starts with an empty jar.': 'Следующий запрос начнёт с пустого хранилища cookie.',

  // Updates
  'volt {version} is available': 'Доступен volt {version}',
  'Install it from Settings': 'Установить можно в настройках',
  'volt is up to date': 'Установлена последняя версия volt',
  'You are on {version}': 'Текущая версия: {version}',
  'Could not check for updates: {error}': 'Не удалось проверить обновления: {error}',

  // Environments and workspaces
  'Delete “{name}”? Its environment file goes, and with it any secret values saved for it.':
    'Удалить «{name}»? Файл окружения будет удалён, а с ним и сохранённые для него секретные значения.',
  'Delete environment': 'Удалить окружение',
  'A workspace needs a name.': 'Рабочему пространству нужно имя.',
  'There is already a workspace called “{name}”.': 'Рабочее пространство с именем «{name}» уже есть.',
  'Remove the workspace “{name}”? The collections in it are left exactly where they are.':
    'Убрать рабочее пространство «{name}»? Его коллекции останутся там же, где лежат.',
  'Remove workspace': 'Убрать рабочее пространство',
  'Remove': 'Удалить',
  'Templates written': 'Шаблоны записаны',
  'Nothing to template': 'Не из чего делать шаблоны',
  'No environment has a secret in it yet': 'Пока ни в одном окружении нет секретов',

  // Monitors, mock, sync
  'Test failed: {from} {detail}': 'Тест не пройден: {from} {detail}',
  '{op} {expected}, got {actual}': '{op} {expected}, получено: {actual}',
  'nothing': 'ничего',
  '{name} failed': '{name}: сбой',
  'Answered {status}': 'Получен ответ {status}',
  'Mock is serving': 'Мок-сервер работает',
  'Pulled': 'Pull выполнен',
  'Committed and pushed': 'Commit и push выполнены',
  'Committed': 'Commit выполнен',

  // Connections and history
  'Another request is connected. Disconnect it first.': 'Подключён другой запрос. Сначала отключите его.',
  'Nothing is connected.': 'Ничего не подключено.',
  'That green mark is a test': 'Эта зелёная отметка — тест',
  'status is 200, on the Tests tab. A failing test is what makes a run fail — here and from volt-run in CI.':
    'status is 200, на вкладке «Тесты». Запуск считается неудачным, когда не пройден тест, — и здесь, и в volt-run в CI.',
  'This request failed when it was sent: {error}': 'При отправке этот запрос завершился ошибкой: {error}',
  'Nothing from history is open.': 'Из истории ничего не открыто.',
  'Clear the request history for this collection? This cannot be undone.':
    'Очистить историю запросов этой коллекции? Это действие нельзя отменить.',
  'Clear history': 'Очистить историю',
  'Clear': 'Очистить',

  // The error strip: what went wrong, in the user's words
  'Cancelled.': 'Отменено.',
  '{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    '{host} прислал сертификат, которому volt не доверяет: самоподписанный, просроченный или выданный на другое имя. Если это ваш сервер, отправьте запрос без проверки.',
  'The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    'Сервер прислал сертификат, которому volt не доверяет: самоподписанный, просроченный или выданный на другое имя. Если это ваш сервер, отправьте запрос без проверки.',
  'Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.':
    'Не удалось подключиться к {host}: там никто не слушает. Проверьте URL и убедитесь, что сервер запущен.',
  'Could not connect to the server: nothing is listening there. Check the URL and that the server is running.':
    'Не удалось подключиться к серверу: там никто не слушает. Проверьте URL и убедитесь, что сервер запущен.',
  'Could not find {host}. Check the URL for a typo, and that you are online.':
    'Не удалось найти {host}. Проверьте, нет ли в URL опечатки и есть ли подключение к сети.',
  'Could not find the server. Check the URL for a typo, and that you are online.':
    'Не удалось найти сервер. Проверьте, нет ли в URL опечатки и есть ли подключение к сети.',
  'No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Нет ответа от {host} за {seconds} с. Возможно, сервер работает медленно или недоступен; тайм-аут задаётся в настройках, а для отдельного запроса — на его вкладке «Настройки».',
  'No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Нет ответа от сервера за {seconds} с. Возможно, сервер работает медленно или недоступен; тайм-аут задаётся в настройках, а для отдельного запроса — на его вкладке «Настройки».',
  'No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Нет ответа от {host}. Возможно, сервер работает медленно или недоступен; тайм-аут задаётся в настройках, а для отдельного запроса — на его вкладке «Настройки».',
  'No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Нет ответа от сервера. Возможно, сервер работает медленно или недоступен; тайм-аут задаётся в настройках, а для отдельного запроса — на его вкладке «Настройки».',
  '{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    '{host} закрыл соединение, не ответив. Попробуйте ещё раз; если это повторяется, сервер отклоняет запрос ещё до обработки.',
  'The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    'Сервер закрыл соединение, не ответив. Попробуйте ещё раз; если это повторяется, сервер отклоняет запрос ещё до обработки.',
  'No route to {host}. Check your network connection or proxy.':
    'Нет маршрута до {host}. Проверьте сетевое подключение или прокси.',
  'No route to the server. Check your network connection or proxy.':
    'Нет маршрута до сервера. Проверьте сетевое подключение или прокси.',
  'The proxy refused or could not reach {host}. Check the proxy in Settings.':
    'Прокси отклонил запрос или не смог связаться с {host}. Проверьте прокси в настройках.',
  'The proxy refused or could not reach the server. Check the proxy in Settings.':
    'Прокси отклонил запрос или не смог связаться с сервером. Проверьте прокси в настройках.',
  'That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.':
    'На такой URL volt отправить запрос не может. Нужны схема и хост, например https://api.example.com/path.',

  // The JSON path filter
  'Cannot read the path from “{rest}”': 'Не удаётся разобрать путь начиная с «{rest}»',
} satisfies Record<string, string>
