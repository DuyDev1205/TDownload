//! Ngôn ngữ cho các thông báo do phần Rust tạo ra (chủ yếu là lỗi). Giao diện báo ngôn ngữ đang chọn qua lệnh
//! `set_language`; mặc định Tiếng Việt. Mỗi thông báo có đủ 6 bản theo thứ tự [vi, en, es, de, zh, ja].

use std::sync::atomic::{AtomicUsize, Ordering};

type Text = [&'static str; 6];

static LANG: AtomicUsize = AtomicUsize::new(0);

#[tauri::command]
pub fn set_language(lang: String) {
    let i = ["vi", "en", "es", "de", "zh", "ja"].iter().position(|l| *l == lang).unwrap_or(0);
    LANG.store(i, Ordering::Relaxed);
}

/// Bản dịch theo ngôn ngữ đang chọn.
pub fn t(text: Text) -> &'static str {
    text[LANG.load(Ordering::Relaxed)]
}

// Cài thành phần (deps.rs). {name}: tên gói, {e}: lỗi gốc, {files}: danh sách file.
pub const DOWNLOAD_FAILED: Text = [
    "Không tải được {name}: {e}",
    "Couldn't download {name}: {e}",
    "No se pudo descargar {name}: {e}",
    "{name} konnte nicht heruntergeladen werden: {e}",
    "无法下载 {name}：{e}",
    "{name} をダウンロードできませんでした：{e}",
];
pub const CONNECTION_LOST: Text = [
    "Mất kết nối khi tải {name}: {e}",
    "Connection lost while downloading {name}: {e}",
    "Se perdió la conexión al descargar {name}: {e}",
    "Verbindung beim Herunterladen von {name} unterbrochen: {e}",
    "下载 {name} 时连接中断：{e}",
    "{name} のダウンロード中に接続が切れました：{e}",
];
pub const MISSING_FILES: Text = [
    "Gói tải về thiếu file: {files}",
    "The downloaded package is missing files: {files}",
    "Al paquete descargado le faltan archivos: {files}",
    "Im heruntergeladenen Paket fehlen Dateien: {files}",
    "下载的安装包缺少文件：{files}",
    "ダウンロードしたパッケージにファイルが足りません：{files}",
];

// Đọc link và tải (download.rs).
pub const RUN_YTDLP: Text = [
    "Không chạy được yt-dlp: {e}",
    "Couldn't run yt-dlp: {e}",
    "No se pudo ejecutar yt-dlp: {e}",
    "yt-dlp konnte nicht gestartet werden: {e}",
    "无法运行 yt-dlp：{e}",
    "yt-dlp を実行できませんでした：{e}",
];
pub const CHANNEL_HOME: Text = [
    "Đây là trang chủ của kênh. Mở tab Videos (link kết thúc bằng /videos) rồi copy lại link.",
    "This is the channel's home page. Open the Videos tab (link ending in /videos) and copy the link again.",
    "Esta es la página principal del canal. Abre la pestaña Vídeos (enlace terminado en /videos) y copia el enlace de nuevo.",
    "Das ist die Startseite des Kanals. Öffne den Tab „Videos“ (Link endet auf /videos) und kopiere den Link erneut.",
    "这是频道主页。请打开“视频”标签页（链接以 /videos 结尾）后重新复制链接。",
    "これはチャンネルのホームページです。「動画」タブ（リンクの末尾が /videos）を開いてリンクをコピーし直してください。",
];
pub const EMPTY_PLAYLIST: Text = [
    "Danh sách này không có video nào tải được.",
    "This list has no downloadable videos.",
    "Esta lista no tiene vídeos que se puedan descargar.",
    "Diese Liste enthält keine herunterladbaren Videos.",
    "此列表中没有可下载的视频。",
    "このリストにはダウンロードできる動画がありません。",
];
pub const PLAYLIST_TITLE: Text = ["Danh sách phát", "Playlist", "Lista de reproducción", "Playlist", "播放列表", "再生リスト"];
pub const NO_TITLE: Text = ["Không có tiêu đề", "Untitled", "Sin título", "Ohne Titel", "无标题", "タイトルなし"];
pub const SUBTITLE_BLOCKED: Text = [
    "Không tải được phụ đề (trang tạm chặn, thử lại sau).",
    "Couldn't download the subtitles (the site is temporarily blocking them, try again later).",
    "No se pudieron descargar los subtítulos (el sitio los bloquea temporalmente, inténtalo más tarde).",
    "Untertitel konnten nicht geladen werden (die Seite blockiert sie vorübergehend, später erneut versuchen).",
    "无法下载字幕（网站暂时限制，请稍后再试）。",
    "字幕をダウンロードできませんでした（サイトが一時的にブロックしています。後でもう一度お試しください）。",
];
pub const UNKNOWN_ERROR: Text =
    ["Lỗi không xác định", "Unknown error", "Error desconocido", "Unbekannter Fehler", "未知错误", "不明なエラー"];

// Gợi ý cho các lỗi yt-dlp hay gặp.
pub const HINT_BROWSER_COOKIES: Text = [
    "Không đọc được cookie từ trình duyệt (Chrome/Edge bản mới khoá cookie lại). Video công khai thì vào Cài đặt → Cookie đăng nhập → Không dùng. Cần đăng nhập thì dùng Firefox hoặc file cookies.txt.",
    "Couldn't read the browser's cookies (recent Chrome/Edge lock them). For public videos, go to Settings → Login cookies → Don't use. If you need to sign in, use Firefox or a cookies.txt file.",
    "No se pudieron leer las cookies del navegador (las versiones recientes de Chrome/Edge las bloquean). Para vídeos públicos, ve a Ajustes → Cookies de sesión → No usar. Si necesitas iniciar sesión, usa Firefox o un archivo cookies.txt.",
    "Die Browser-Cookies konnten nicht gelesen werden (neuere Chrome/Edge-Versionen sperren sie). Bei öffentlichen Videos: Einstellungen → Login-Cookies → Nicht verwenden. Ist eine Anmeldung nötig, nutze Firefox oder eine cookies.txt-Datei.",
    "无法读取浏览器 Cookie（新版 Chrome/Edge 会锁定 Cookie）。公开视频请前往设置 → 登录 Cookie → 不使用。需要登录时请使用 Firefox 或 cookies.txt 文件。",
    "ブラウザの Cookie を読み取れませんでした（最近の Chrome/Edge は Cookie をロックします）。公開動画なら 設定 → ログイン Cookie → 使用しない にしてください。ログインが必要なら Firefox か cookies.txt ファイルを使ってください。",
];
pub const HINT_LOGIN: Text = [
    "Video cần đăng nhập hoặc là video riêng tư. Vào Cài đặt → Cookie đăng nhập, chọn trình duyệt bạn đã đăng nhập.",
    "This video requires signing in or is private. Go to Settings → Login cookies and pick a browser you're signed in on.",
    "Este vídeo requiere iniciar sesión o es privado. Ve a Ajustes → Cookies de sesión y elige un navegador con la sesión iniciada.",
    "Dieses Video erfordert eine Anmeldung oder ist privat. Wähle unter Einstellungen → Login-Cookies einen Browser, in dem du angemeldet bist.",
    "该视频需要登录或为私密视频。请前往设置 → 登录 Cookie，选择已登录的浏览器。",
    "この動画はログインが必要か、非公開です。設定 → ログイン Cookie で、ログイン済みのブラウザを選んでください。",
];
pub const HINT_NETWORK: Text = [
    "Không kết nối được tới trang web. Kiểm tra mạng (hoặc VPN/proxy) rồi thử lại.",
    "Couldn't connect to the site. Check your network (or VPN/proxy) and try again.",
    "No se pudo conectar con el sitio. Revisa la red (o la VPN/proxy) e inténtalo de nuevo.",
    "Keine Verbindung zur Website. Prüfe das Netzwerk (oder VPN/Proxy) und versuche es erneut.",
    "无法连接到网站。请检查网络（或 VPN/代理）后重试。",
    "サイトに接続できませんでした。ネットワーク（または VPN/プロキシ）を確認して、もう一度お試しください。",
];
pub const HINT_TIKTOK_PROFILE: Text = [
    "yt-dlp hiện chưa tải được cả trang cá nhân TikTok. Hãy copy link từng video (có thể dán nhiều link một lúc).",
    "yt-dlp can't download a whole TikTok profile yet. Copy each video's link instead (you can paste several at once).",
    "yt-dlp aún no puede descargar un perfil completo de TikTok. Copia el enlace de cada vídeo (puedes pegar varios a la vez).",
    "yt-dlp kann noch kein ganzes TikTok-Profil laden. Kopiere stattdessen die Links der einzelnen Videos (mehrere auf einmal einfügen geht).",
    "yt-dlp 目前还无法下载整个 TikTok 个人主页。请逐个复制视频链接（可以一次粘贴多个）。",
    "yt-dlp はまだ TikTok のプロフィール全体をダウンロードできません。動画ごとのリンクをコピーしてください（複数まとめて貼り付けできます）。",
];
pub const HINT_UNSUPPORTED: Text = [
    "Link này không được hỗ trợ.",
    "This link isn't supported.",
    "Este enlace no es compatible.",
    "Dieser Link wird nicht unterstützt.",
    "不支持此链接。",
    "このリンクには対応していません。",
];
pub const HINT_FFMPEG: Text = [
    "Lỗi khi ghép/chuyển đổi file. Thử Cài đặt → Cài lại thành phần.",
    "Error while merging/converting the file. Try Settings → Reinstall components.",
    "Error al unir/convertir el archivo. Prueba Ajustes → Reinstalar componentes.",
    "Fehler beim Zusammenfügen/Umwandeln der Datei. Versuche Einstellungen → Komponenten neu installieren.",
    "合并/转换文件时出错。请尝试设置 → 重新安装组件。",
    "ファイルの結合/変換中にエラーが発生しました。設定 → コンポーネントを再インストール をお試しください。",
];
pub const HINT_SITE_CHANGED: Text = [
    "Trang web có thể vừa thay đổi. Thử Cài đặt → Cập nhật yt-dlp rồi tải lại.",
    "The site may have just changed. Try Settings → Update yt-dlp, then download again.",
    "Puede que el sitio acabe de cambiar. Prueba Ajustes → Actualizar yt-dlp y vuelve a descargar.",
    "Die Website hat sich vielleicht gerade geändert. Versuche Einstellungen → yt-dlp aktualisieren und lade erneut.",
    "网站可能刚刚有改动。请尝试设置 → 更新 yt-dlp 后重新下载。",
    "サイトが変更された可能性があります。設定 → yt-dlp をアップデート してから、もう一度ダウンロードしてください。",
];

// Máy chủ góp ý (feedback.rs).
pub const FEEDBACK_BAD_URL: Text = [
    "Địa chỉ máy chủ góp ý không hợp lệ.",
    "Invalid feedback server address.",
    "La dirección del servidor de sugerencias no es válida.",
    "Ungültige Adresse des Feedback-Servers.",
    "反馈服务器地址无效。",
    "フィードバックサーバーのアドレスが正しくありません。",
];
pub const FEEDBACK_CONNECT: Text = [
    "Không kết nối được máy chủ góp ý: {e}",
    "Couldn't connect to the feedback server: {e}",
    "No se pudo conectar con el servidor de sugerencias: {e}",
    "Keine Verbindung zum Feedback-Server: {e}",
    "无法连接反馈服务器：{e}",
    "フィードバックサーバーに接続できませんでした：{e}",
];
pub const FEEDBACK_BAD_DATA: Text = [
    "Máy chủ góp ý trả về dữ liệu lạ (đã triển khai Apps Script đúng cách chưa?).",
    "The feedback server returned unexpected data (is the Apps Script deployed correctly?).",
    "El servidor de sugerencias devolvió datos inesperados (¿se implementó bien el Apps Script?).",
    "Der Feedback-Server hat unerwartete Daten geliefert (ist das Apps Script richtig bereitgestellt?).",
    "反馈服务器返回了异常数据（Apps Script 是否已正确部署？）。",
    "フィードバックサーバーから予期しないデータが返されました（Apps Script は正しくデプロイされていますか？）。",
];
pub const FEEDBACK_SERVER_ERROR: Text = [
    "Máy chủ góp ý báo lỗi.",
    "The feedback server reported an error.",
    "El servidor de sugerencias informó de un error.",
    "Der Feedback-Server hat einen Fehler gemeldet.",
    "反馈服务器报告了错误。",
    "フィードバックサーバーでエラーが発生しました。",
];
