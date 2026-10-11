/** Turkish for what the store says: toasts, confirmations, file dialogs, and the error strip's sentences. */
export default {
  // Deleting, and what goes with it
  'Delete “{name}”? The file is removed from disk.': '“{name}” silinsin mi? Dosya diskten kaldırılır.',
  'Delete the empty folder “{name}”?': 'Boş klasör “{name}” silinsin mi?',
  '1 request': '1 istek',
  '{n} requests': '{n} istek',
  '1 folder': '1 klasör',
  '{n} folders': '{n} klasör',
  ' and ': ' ve ',
  'Delete “{name}” and the {parts} inside it? This removes them from disk.':
    '“{name}” ve içindeki {parts} silinsin mi? Bunlar diskten kaldırılır.',
  'Delete': 'Sil',
  'Cancel': 'Vazgeç',
  'That item is no longer in the collection.': 'Bu öğe artık koleksiyonda değil.',
  'It is in the bin': 'Çöp kutusunda duruyor',
  'It is in the bin · 1 monitor stopped': 'Çöp kutusunda duruyor · 1 izleyici durduruldu',
  'It is in the bin · {n} monitors stopped': 'Çöp kutusunda duruyor · {n} izleyici durduruldu',
  'Deleted {name}': '{name} silindi',
  '{name} is back': '{name} geri geldi',
  'Empty the bin? What is in it is gone for good.': 'Çöp kutusu boşaltılsın mı? İçindekiler kalıcı olarak silinir.',
  'Empty the bin': 'Çöp kutusunu boşalt',
  'Empty it': 'Boşalt',

  // Collections and files
  'No collection is open. Use “Open…” or “New” first.': 'Açık bir koleksiyon yok. Önce “Aç…” veya “Yeni” ile başlayın.',
  'This file could not be read and is not in the tree: {files}': 'Bu dosya okunamadı ve ağaçta yer almıyor: {files}',
  'These files could not be read and are not in the tree: {files}': 'Bu dosyalar okunamadı ve ağaçta yer almıyor: {files}',
  'Moved to {where}': '{where} koleksiyonuna taşındı',
  '{id} there; the copy here is in the bin': 'Orada {id}; buradaki kopya çöp kutusunda',
  'Copied to {where}': '{where} koleksiyonuna kopyalandı',
  'as {id}': '{id} olarak',
  "Choose an export: Postman, Insomnia, OpenAPI, HAR, a Bruno collection's bruno.json, or a Postman environment":
    'Bir dışa aktarım seçin: Postman, Insomnia, OpenAPI, HAR, bir Bruno koleksiyonunun bruno.json dosyası veya bir Postman ortamı',
  'Postman, Insomnia, Bruno, OpenAPI, HAR or environment': 'Postman, Insomnia, Bruno, OpenAPI, HAR veya ortam',
  'Choose where to create the collection': 'Koleksiyonun oluşturulacağı yeri seçin',
  'Environment {name} imported': '{name} ortamı içe aktarıldı',
  'Secret values went to the gitignored .env file.': 'Gizli değerler, git’in yok saydığı .env dosyasına yazıldı.',
  'Export as an OpenAPI document': 'OpenAPI belgesi olarak dışa aktar',
  'Exported as OpenAPI': 'OpenAPI olarak dışa aktarıldı',
  'Export as Postman collection': 'Postman koleksiyonu olarak dışa aktar',
  'Exported for Postman': 'Postman için dışa aktarıldı',
  'Write the documentation': 'Belgeleri yaz',
  'Documentation written': 'Belgeler yazıldı',

  // Tabs and requests
  'this request': 'bu istek',
  'Close “{name}”? Its unsaved changes are lost.': '“{name}” kapatılsın mı? Kaydedilmemiş değişiklikler kaybolur.',
  'Close': 'Kapat',
  'Keep open': 'Açık kalsın',
  'Open a request to fill it from curl.': 'curl’den doldurmak için bir istek açın.',
  'Open a request to copy it as curl.': 'curl olarak kopyalamak için bir istek açın.',
  'Secrets kept as {names}': 'Gizli değerler {names} olarak bırakıldı',
  'No value for {names}': 'Değeri olmayanlar: {names}',
  'Copied as cURL, with secret values': 'cURL olarak kopyalandı, gizli değerlerle birlikte',
  'Copied as cURL': 'cURL olarak kopyalandı',
  'Nothing to save.': 'Kaydedilecek bir şey yok.',
  'Open a request first.': 'Önce bir istek açın.',
  'Folder name cannot be empty.': 'Klasör adı boş olamaz.',
  'Name cannot be empty.': 'Ad boş olamaz.',
  'That row is no longer where it was; try again.': 'Bu satır artık eski yerinde değil; yeniden deneyin.',

  // Captures, examples, cookies
  'Already captured': 'Zaten yakalanıyor',
  '{from} is on the Captures tab': '{from} Yakalamalar sekmesinde',
  'Capture added: {name}': 'Yakalama eklendi: {name}',
  '{from} — send again to fill it': '{from} — doldurmak için yeniden gönderin',
  '{from} — send again to fill it, kept as a secret': '{from} — doldurmak için yeniden gönderin; gizli değer olarak saklanır',
  'Captured {name}': '{name} yakalandı',
  'Captured {n} values': '{n} değer yakalandı',
  'Saved in {name}': '{name} ortamına kaydedildi',
  'Send a request first, then keep its response.': 'Önce bir istek gönderin, sonra yanıtını saklayın.',
  'Example saved': 'Örnek kaydedildi',
  'Kept as “{name}” beside the request': 'İsteğin yanında “{name}” adıyla saklandı',
  'Cookies cleared': 'Çerezler temizlendi',
  'The next request starts with an empty jar.': 'Sonraki istek boş bir çerez kavanozuyla başlar.',

  // Updates
  'volt {version} is available': 'volt {version} yayımlandı',
  'Install it from Settings': 'Ayarlar’dan kurabilirsiniz',
  'volt is up to date': 'volt güncel',
  'You are on {version}': 'Kullandığınız sürüm: {version}',
  'Could not check for updates: {error}': 'Güncellemeler denetlenemedi: {error}',

  // Environments and workspaces
  'Delete “{name}”? Its environment file goes, and with it any secret values saved for it.':
    '“{name}” silinsin mi? Ortam dosyası, onun için kaydedilmiş gizli değerlerle birlikte silinir.',
  'Delete environment': 'Ortamı sil',
  'A workspace needs a name.': 'Çalışma alanına bir ad verin.',
  'There is already a workspace called “{name}”.': '“{name}” adında bir çalışma alanı zaten var.',
  'Remove the workspace “{name}”? The collections in it are left exactly where they are.':
    '“{name}” çalışma alanı kaldırılsın mı? İçindeki koleksiyonlar oldukları yerde kalır.',
  'Remove workspace': 'Çalışma alanını kaldır',
  'Remove': 'Kaldır',
  'Templates written': 'Şablonlar yazıldı',
  'Nothing to template': 'Şablonu çıkarılacak bir şey yok',
  'No environment has a secret in it yet': 'Henüz hiçbir ortamda gizli değer yok',

  // Monitors, mock, sync
  'Test failed: {from} {detail}': 'Test başarısız: {from} {detail}',
  '{op} {expected}, got {actual}': '{op} {expected} bekleniyordu, gelen: {actual}',
  'nothing': 'hiçbir şey',
  '{name} failed': '{name} başarısız oldu',
  'Answered {status}': '{status} yanıtı geldi',
  'Mock is serving': 'Sahte sunucu çalışıyor',
  'Pulled': 'Çekildi',
  'Committed and pushed': 'İşlendi ve gönderildi',
  'Committed': 'İşlendi',

  // Connections and history
  'Another request is connected. Disconnect it first.': 'Başka bir istek bağlı. Önce onun bağlantısını kesin.',
  'Nothing is connected.': 'Bağlı bir şey yok.',
  'That green mark is a test': 'Bu yeşil işaret bir test',
  'status is 200, on the Tests tab. A failing test is what makes a run fail — here and from volt-run in CI.':
    'status is 200, Testler sekmesinde. Bir çalıştırmayı başarısız kılan şey, başarısız bir testtir — hem burada hem de CI’daki volt-run’da.',
  'This request failed when it was sent: {error}': 'Bu istek gönderildiğinde başarısız olmuştu: {error}',
  'Nothing from history is open.': 'Geçmişten açılmış bir istek yok.',
  'Clear the request history for this collection? This cannot be undone.':
    'Bu koleksiyonun istek geçmişi temizlensin mi? Bu işlem geri alınamaz.',
  'Clear history': 'Geçmişi temizle',
  'Clear': 'Temizle',

  // The error strip: what went wrong, in the user's words
  'Cancelled.': 'İptal edildi.',
  '{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    '{host}, volt’un güvenmediği bir sertifika gönderdi: kendinden imzalı, süresi dolmuş ya da başka bir ad için düzenlenmiş. Sunucu sizin denetiminizdeyse sertifikayı doğrulamadan gönderin.',
  'The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.':
    'Sunucu, volt’un güvenmediği bir sertifika gönderdi: kendinden imzalı, süresi dolmuş ya da başka bir ad için düzenlenmiş. Sunucu sizin denetiminizdeyse sertifikayı doğrulamadan gönderin.',
  'Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.':
    '{host} adresine bağlanılamadı: orada dinleyen bir şey yok. URL’yi ve sunucunun çalıştığını denetleyin.',
  'Could not connect to the server: nothing is listening there. Check the URL and that the server is running.':
    'Sunucuya bağlanılamadı: orada dinleyen bir şey yok. URL’yi ve sunucunun çalıştığını denetleyin.',
  'Could not find {host}. Check the URL for a typo, and that you are online.':
    '{host} bulunamadı. URL’de yazım hatası olup olmadığını ve çevrimiçi olduğunuzu denetleyin.',
  'Could not find the server. Check the URL for a typo, and that you are online.':
    'Sunucu bulunamadı. URL’de yazım hatası olup olmadığını ve çevrimiçi olduğunuzu denetleyin.',
  'No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    '{host} {seconds} sn içinde yanıt vermedi. Sunucu yavaş ya da erişilemez olabilir; zaman aşımı Ayarlar’da, istek başına ise isteğin Ayarlar sekmesinde belirlenir.',
  'No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sunucu {seconds} sn içinde yanıt vermedi. Sunucu yavaş ya da erişilemez olabilir; zaman aşımı Ayarlar’da, istek başına ise isteğin Ayarlar sekmesinde belirlenir.',
  'No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    '{host} yanıt vermedi. Sunucu yavaş ya da erişilemez olabilir; zaman aşımı Ayarlar’da, istek başına ise isteğin Ayarlar sekmesinde belirlenir.',
  'No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.':
    'Sunucu yanıt vermedi. Sunucu yavaş ya da erişilemez olabilir; zaman aşımı Ayarlar’da, istek başına ise isteğin Ayarlar sekmesinde belirlenir.',
  '{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    '{host} yanıt vermeden bağlantıyı kapattı. Yeniden deneyin; sürekli oluyorsa sunucu isteği erkenden reddediyordur.',
  'The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.':
    'Sunucu yanıt vermeden bağlantıyı kapattı. Yeniden deneyin; sürekli oluyorsa sunucu isteği erkenden reddediyordur.',
  'No route to {host}. Check your network connection or proxy.':
    '{host} adresine giden bir yol yok. Ağ bağlantınızı veya proxy’yi denetleyin.',
  'No route to the server. Check your network connection or proxy.':
    'Sunucuya giden bir yol yok. Ağ bağlantınızı veya proxy’yi denetleyin.',
  'The proxy refused or could not reach {host}. Check the proxy in Settings.':
    'Proxy isteği reddetti ya da {host} adresine ulaşamadı. Ayarlar’daki proxy’yi denetleyin.',
  'The proxy refused or could not reach the server. Check the proxy in Settings.':
    'Proxy isteği reddetti ya da sunucuya ulaşamadı. Ayarlar’daki proxy’yi denetleyin.',
  'That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.':
    'Bu, volt’un istek gönderebileceği bir URL değil. https://api.example.com/path gibi bir şema ve bir sunucu adı gerekir.',

  // The JSON path filter
  'Cannot read the path from “{rest}”': 'Yol şuradan itibaren okunamıyor: “{rest}”',
} satisfies Record<string, string>
