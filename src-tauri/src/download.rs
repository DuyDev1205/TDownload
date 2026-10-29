//! Lấy thông tin video và tải video/MP3 bằng yt-dlp, báo tiến độ lên giao diện qua event.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::deps::{self, err};
use crate::proxy;

const OUTPUT_TEMPLATE: &str = "%(title).150B [%(id)s].%(ext)s";
// Người dùng đã tự đặt tên thì giữ đúng tên đó, không thêm mã video.
const CUSTOM_OUTPUT_TEMPLATE: &str = "%(title).150B.%(ext)s";

// Ưu tiên H.264 + AAC tối đa 1080p (mở được trên mọi máy), không có thì lấy bản tốt nhất ≤ 1080p.
const COMPAT_FORMAT: &str = "bv*[vcodec~='^(avc|h264)'][height<=1080]+ba[ext=m4a]\
    /b[vcodec~='^(avc|h264)'][height<=1080]\
    /bv*[height<=1080]+ba/b[height<=1080]/b";

// Mỗi dòng tiến độ có dạng DL|format|đã tải|tổng|tổng ước tính|tốc độ|còn lại|mảnh hiện tại|số mảnh.
const DOWNLOAD_PROGRESS: &str = "download:DL|%(info.format_id)s|%(progress.downloaded_bytes)s\
    |%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s\
    |%(progress.fragment_index)s|%(progress.fragment_count)s";
const POSTPROCESS_PROGRESS: &str = "postprocess:PP|%(progress.postprocessor)s|%(progress.status)s";

struct Job {
    pid: u32,
    canceled: bool,
}

#[derive(Default)]
pub struct Jobs(Mutex<HashMap<String, Job>>);

impl Jobs {
    pub fn kill_all(&self) {
        for job in self.0.lock().unwrap().values() {
            kill_tree(job.pid);
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Best,
    Compat,
    Audio,
}

/// Định dạng khi chỉ lấy âm thanh. Các trang chỉ phát âm thanh đã nén, nên FLAC/WAV giữ trọn chất lượng bản gốc
/// (không nén thêm lần nữa) chứ không làm hay hơn bản gốc.
#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    /// Giữ nguyên luồng âm thanh gốc (opus/m4a/mp3), không mã hoá lại.
    Original,
    Mp3,
    Flac,
    Wav,
}

fn mode_args(mode: Mode, audio: AudioFormat) -> Vec<&'static str> {
    match mode {
        Mode::Best => vec!["-f", "bv*+ba/b", "--merge-output-format", "mp4", "--remux-video", "mp4"],
        Mode::Compat => vec!["-f", COMPAT_FORMAT, "--merge-output-format", "mp4", "--remux-video", "mp4"],
        Mode::Audio => {
            let mut args = vec!["-f", "ba/b", "-x", "--audio-format"];
            args.push(match audio {
                AudioFormat::Original => "best",
                AudioFormat::Mp3 => "mp3",
                AudioFormat::Flac => "flac",
                AudioFormat::Wav => "wav",
            });
            if audio == AudioFormat::Mp3 {
                args.extend(["--audio-quality", "320K"]);
            }
            // WAV không chứa được ảnh bìa, yt-dlp sẽ báo lỗi nếu cố nhúng.
            if audio != AudioFormat::Wav {
                args.push("--embed-thumbnail");
            }
            args
        }
    }
}

/// Chọn bản có độ phân giải (cạnh ngắn) lớn nhất không vượt quá `quality`; không có thì lấy bản nhỏ nhất trên mức đó.
fn quality_args(mode: Mode, quality: Option<u32>) -> Vec<String> {
    match (mode, quality) {
        (Mode::Audio, _) | (_, None) => vec![],
        (_, Some(q)) => vec!["-S".into(), format!("res:{q}")],
    }
}

/// Đổi tiêu đề trước khi đặt tên file: yt-dlp tự thay ký tự cấm trong tên file và ghi tiêu đề mới vào file.
/// Chuỗi thay thế đi qua re.sub của Python nên phải nhân đôi dấu `\`.
fn title_args(title: Option<&str>) -> Vec<String> {
    match title {
        Some(t) => vec!["--replace-in-metadata".into(), "title".into(), "(?s).+".into(), t.replace('\\', "\\\\")],
        None => vec![],
    }
}

/// `cookies` là "" (không dùng), "browser:<tên trình duyệt>" hoặc "file:<đường dẫn cookies.txt>".
fn cookie_args(cookies: &str) -> Vec<String> {
    if let Some(browser) = cookies.strip_prefix("browser:") {
        vec!["--cookies-from-browser".into(), browser.into()]
    } else if let Some(file) = cookies.strip_prefix("file:") {
        vec!["--cookies".into(), file.into()]
    } else {
        vec![]
    }
}

/// Phụ đề tải kèm video.
#[derive(Clone, Serialize, Deserialize)]
pub struct Subtitle {
    /// Mã ngôn ngữ đúng như yt-dlp liệt kê, ví dụ "en" hoặc "en-orig".
    lang: String,
    name: String,
    /// Phụ đề tự động (nhận dạng giọng nói) thay vì do người đăng làm.
    auto: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    title: String,
    thumbnail: Option<String>,
    duration: Option<f64>,
    uploader: Option<String>,
    site: String,
    max_height: Option<u64>,
    /// Các độ phân giải có sẵn, giảm dần. Rỗng nghĩa là chỉ có âm thanh (ví dụ SoundCloud).
    heights: Vec<u64>,
    subtitles: Vec<Subtitle>,
    url: String,
}

#[derive(Serialize)]
pub struct PlaylistEntry {
    url: String,
    title: String,
    duration: Option<f64>,
    thumbnail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    title: String,
    uploader: Option<String>,
    site: String,
    /// Tổng số mục của danh sách nếu trang cho biết (có thể nhiều hơn số mục lấy về).
    count: Option<u64>,
    entries: Vec<PlaylistEntry>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FetchResult {
    Video(VideoInfo),
    Playlist(PlaylistInfo),
}

/// Số mục tối đa lấy về khi dán link playlist/kênh, để kênh có hàng nghìn video vẫn mở nhanh.
const PLAYLIST_LIMIT: u32 = 300;

#[tauri::command]
pub async fn fetch_info(app: AppHandle, url: String, cookies: String) -> Result<FetchResult, String> {
    // --no-playlist: link video kèm &list=... vẫn là một video; link chỉ là playlist/kênh thì trả về danh sách.
    // --flat-playlist: chỉ đọc danh sách (tiêu đề, link) chứ không mở từng video, nên playlist dài vẫn nhanh.
    let out = deps::command(&app, "yt-dlp")?
        .args(["-J", "--no-playlist", "--flat-playlist", "--no-warnings"])
        .args(["--playlist-end", &PLAYLIST_LIMIT.to_string()])
        .args(proxy::args())
        .args(cookie_args(&cookies))
        .arg("--")
        .arg(&url)
        .output()
        .await
        .map_err(|e| format!("Không chạy được yt-dlp: {e}"))?;
    if !out.status.success() {
        return Err(friendly_error(&String::from_utf8_lossy(&out.stderr)));
    }

    let v: Value = serde_json::from_slice(&out.stdout).map_err(err)?;
    if v["_type"] == "playlist" {
        playlist_info(&v).map(FetchResult::Playlist)
    } else {
        Ok(FetchResult::Video(video_info(&v, url)))
    }
}

fn playlist_info(v: &Value) -> Result<PlaylistInfo, String> {
    let all = v["entries"].as_array().cloned().unwrap_or_default();
    // Link trang chủ kênh YouTube trả về các tab (Videos, Shorts) thay vì video.
    if !all.is_empty() && all.iter().all(|e| e["_type"] == "playlist") {
        return Err("Đây là trang chủ của kênh. Mở tab Videos (link kết thúc bằng /videos) rồi copy lại link.".into());
    }
    let entries: Vec<PlaylistEntry> = all
        .iter()
        .filter_map(|e| {
            let url = e["url"].as_str().or(e["webpage_url"].as_str())?.to_string();
            // SoundCloud không kèm tiêu đề trong danh sách: lấy từ link, ví dụ ".../city-ports" thành "city ports".
            let title = e["title"].as_str().filter(|s| !s.is_empty()).map(str::to_owned).unwrap_or_else(|| {
                url.trim_end_matches('/').rsplit('/').next().unwrap_or(&url).replace(&['-', '_'][..], " ")
            });
            let thumbnail = e["thumbnails"]
                .as_array()
                .and_then(|t| t.last())
                .and_then(|t| t["url"].as_str())
                .or(e["thumbnail"].as_str())
                .map(str::to_owned);
            Some(PlaylistEntry { url, title, duration: e["duration"].as_f64(), thumbnail })
        })
        .collect();
    if entries.is_empty() {
        return Err("Danh sách này không có video nào tải được.".into());
    }
    let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    Ok(PlaylistInfo {
        title: text("title").unwrap_or_else(|| "Danh sách phát".into()),
        uploader: text("uploader").or_else(|| text("channel")),
        site: text("extractor_key").unwrap_or_default(),
        count: v["playlist_count"].as_u64(),
        entries,
    })
}

/// Phụ đề người đăng làm, cộng phụ đề tự động theo ngôn ngữ gốc của video. Bỏ phụ đề dịch máy của YouTube
/// (hàng trăm ngôn ngữ) vì YouTube gần như luôn chặn tải chúng (HTTP 429).
fn subtitles(v: &Value) -> Vec<Subtitle> {
    let list = |key: &str, auto: bool| -> Vec<Subtitle> {
        v[key]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter(|(lang, _)| lang.as_str() != "live_chat")
                    .map(|(lang, tracks)| Subtitle {
                        lang: lang.clone(),
                        name: tracks[0]["name"].as_str().unwrap_or(lang).to_string(),
                        auto,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut subs = list("subtitles", false);
    let autos = list("automatic_captions", true);
    if autos.iter().any(|s| s.lang.ends_with("-orig")) {
        subs.extend(autos.into_iter().filter(|s| s.lang.ends_with("-orig")));
    } else if autos.len() <= 5 {
        subs.extend(autos);
    }
    subs
}

fn video_info(v: &Value, url: String) -> VideoInfo {
    let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    // Độ phân giải tính theo cạnh ngắn như yt-dlp, để video dọc 1080x1920 vẫn là 1080p.
    let res = |f: &Value| match (f["width"].as_u64(), f["height"].as_u64()) {
        (Some(w), Some(h)) => Some(w.min(h)),
        (None, h) => h,
        _ => None,
    };
    let mut heights: Vec<u64> = v["formats"]
        .as_array()
        .map(|formats| {
            formats
                .iter()
                .filter(|f| f["vcodec"].as_str() != Some("none"))
                .filter_map(res)
                .filter(|&h| h >= 144)
                .collect()
        })
        .unwrap_or_default();
    if heights.is_empty() {
        heights.extend(res(v));
    }
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();

    VideoInfo {
        title: text("title").unwrap_or_else(|| "Không có tiêu đề".into()),
        thumbnail: text("thumbnail"),
        duration: v["duration"].as_f64(),
        uploader: text("uploader").or_else(|| text("channel")),
        site: text("extractor_key").unwrap_or_default(),
        max_height: heights.first().copied(),
        heights,
        subtitles: subtitles(v),
        url: text("webpage_url").unwrap_or(url),
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    stage: &'static str,
    part: u32,
    percent: Option<f64>,
    speed: Option<f64>,
    eta: Option<f64>,
    label: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    id: String,
    ok: bool,
    canceled: bool,
    file: Option<String>,
    error: Option<String>,
    /// Ghi chú khi tải xong nhưng có phần phụ bị thiếu (ví dụ phụ đề bị chặn).
    note: Option<String>,
}

#[derive(Default)]
struct LineState {
    format_id: String,
    part: u32,
    file: Option<String>,
    subtitle_failed: bool,
    log: Vec<String>,
}

/// Tải phụ đề kèm theo: video thì gắn vào file và giữ thêm file .srt, âm thanh thì chỉ lưu file .srt.
/// Có `-i` để phụ đề bị chặn (YouTube hay trả HTTP 429) không làm hỏng cả lượt tải; video lỗi thật thì
/// yt-dlp vẫn trả mã lỗi nên app vẫn biết.
fn subtitle_args(mode: Mode, subtitle: Option<&Subtitle>) -> Vec<String> {
    let Some(s) = subtitle else { return vec![] };
    let mut args: Vec<String> = vec![
        if s.auto { "--write-auto-subs" } else { "--write-subs" }.into(),
        "--sub-langs".into(),
        s.lang.clone(),
        "--convert-subs".into(),
        "srt".into(),
        "-i".into(),
    ];
    if !matches!(mode, Mode::Audio) {
        args.push("--embed-subs".into());
    }
    args
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    id: String,
    url: String,
    mode: Mode,
    audio_format: Option<AudioFormat>,
    quality: Option<u32>,
    title: Option<String>,
    subtitle: Option<Subtitle>,
    out_dir: String,
    cookies: String,
) -> Result<(), String> {
    let title = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let bin = deps::bin_dir(&app)?;
    let mut cmd = deps::command(&app, "yt-dlp")?;
    cmd.args(["--no-playlist", "--newline", "--progress", "--no-mtime", "--embed-metadata"])
        .arg("--ffmpeg-location")
        .arg(&bin)
        .arg("-P")
        .arg(&out_dir)
        .args(["-o", if title.is_some() { CUSTOM_OUTPUT_TEMPLATE } else { OUTPUT_TEMPLATE }])
        .args(title_args(title.as_deref()))
        .args(["--progress-template", DOWNLOAD_PROGRESS])
        .args(["--progress-template", POSTPROCESS_PROGRESS])
        .args(["--print", "after_move:FILE|%(filepath)s"])
        .args(proxy::args())
        .args(cookie_args(&cookies))
        .args(mode_args(mode, audio_format.unwrap_or(AudioFormat::Original)))
        .args(quality_args(mode, quality))
        .args(subtitle_args(mode, subtitle.as_ref()))
        .arg("--")
        .arg(&url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Không chạy được yt-dlp: {e}"))?;
    jobs.0.lock().unwrap().insert(id.clone(), Job { pid: child.id().unwrap_or(0), canceled: false });

    let state = Arc::new(Mutex::new(LineState::default()));
    let stdout = pipe(child.stdout.take().unwrap(), app.clone(), id.clone(), state.clone());
    let stderr = pipe(child.stderr.take().unwrap(), app.clone(), id.clone(), state.clone());

    tauri::async_runtime::spawn(async move {
        let success = matches!(child.wait().await, Ok(status) if status.success());
        let _ = stdout.await;
        let _ = stderr.await;

        let canceled = app.state::<Jobs>().0.lock().unwrap().remove(&id).is_some_and(|j| j.canceled);
        let done = {
            let s = state.lock().unwrap();
            Done {
                id,
                ok: success && !canceled,
                canceled,
                file: s.file.clone(),
                error: (!success && !canceled).then(|| friendly_error(&s.log.join("\n"))),
                note: (success && s.subtitle_failed)
                    .then(|| "Không tải được phụ đề (trang tạm chặn, thử lại sau).".to_string()),
            }
        };
        let _ = app.emit("dl-done", done);
    });
    Ok(())
}

fn pipe<R>(reader: R, app: AppHandle, id: String, state: Arc<Mutex<LineState>>) -> tauri::async_runtime::JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => handle_line(&app, &id, &state, String::from_utf8_lossy(&buf).trim()),
            }
        }
    })
}

fn handle_line(app: &AppHandle, id: &str, state: &Mutex<LineState>, line: &str) {
    let mut s = state.lock().unwrap();
    if let Some(rest) = line.strip_prefix("DL|") {
        let f: Vec<&str> = rest.split('|').collect();
        if f.len() < 8 {
            return;
        }
        // Bản chất lượng cao tải hình và tiếng riêng: mỗi lần đổi format là sang phần mới.
        if s.format_id != f[0] {
            s.format_id = f[0].to_owned();
            s.part += 1;
        }
        let num = |v: &str| v.parse::<f64>().ok();
        let percent = match (num(f[1]), num(f[2]).or(num(f[3]))) {
            (Some(done), Some(total)) if total > 0.0 => Some(done / total * 100.0),
            _ => match (num(f[6]), num(f[7])) {
                (Some(i), Some(n)) if n > 0.0 => Some(i / n * 100.0),
                _ => None,
            },
        };
        let _ = app.emit(
            "dl-progress",
            Progress {
                id: id.to_owned(),
                stage: "download",
                part: s.part,
                percent: percent.map(|p| p.min(100.0)),
                speed: num(f[4]),
                eta: num(f[5]),
                label: None,
            },
        );
    } else if let Some(rest) = line.strip_prefix("PP|") {
        let _ = app.emit(
            "dl-progress",
            Progress {
                id: id.to_owned(),
                stage: "process",
                part: s.part,
                percent: None,
                speed: None,
                eta: None,
                label: rest.split('|').next().map(str::to_owned),
            },
        );
    } else if let Some(path) = line.strip_prefix("FILE|") {
        s.file = Some(path.to_owned());
    } else if !line.is_empty() {
        if line.contains("Unable to download video subtitles") {
            s.subtitle_failed = true;
        }
        s.log.push(line.to_owned());
        if s.log.len() > 50 {
            s.log.remove(0);
        }
    }
}

/// Kiểm tra các file trong lịch sử tải còn ở chỗ cũ không (người dùng có thể đã xoá hoặc chuyển đi).
#[tauri::command]
pub fn files_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| std::path::Path::new(p).exists()).collect()
}

#[tauri::command]
pub fn cancel_download(jobs: State<'_, Jobs>, id: String) {
    let pid = match jobs.0.lock().unwrap().get_mut(&id) {
        Some(job) => {
            job.canceled = true;
            job.pid
        }
        None => return,
    };
    std::thread::spawn(move || kill_tree(pid));
}

/// Dừng yt-dlp và cả các tiến trình con của nó (ffmpeg, ...).
fn kill_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(deps::CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
}

/// Lấy dòng lỗi cuối của yt-dlp và thêm gợi ý dễ hiểu cho các lỗi hay gặp.
fn friendly_error(log: &str) -> String {
    let detail = log
        .lines()
        .rev()
        .find(|l| l.contains("ERROR"))
        .or_else(|| log.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("Lỗi không xác định")
        .trim()
        .to_string();
    let low = detail.to_lowercase();

    // Chrome/Edge bản mới khoá cookie (app-bound encryption), lỗi báo về chỉ có chữ "DPAPI" chứ không có "cookie".
    let browser_cookie_error = ["dpapi", "app-bound", "app bound"].iter().any(|k| low.contains(k))
        || (low.contains("cookie")
            && ["chrome", "edge", "brave", "decrypt", "could not copy"].iter().any(|k| low.contains(k)));
    let hint = if browser_cookie_error {
        "Không đọc được cookie từ trình duyệt (Chrome/Edge bản mới khoá cookie lại). Video công khai thì vào Cài đặt → \
         Cookie đăng nhập → Không dùng. Cần đăng nhập thì dùng Firefox hoặc file cookies.txt."
    } else if ["login", "log in", "sign in", "cookies", "private", "registered users", "not a bot"].iter().any(|k| low.contains(k)) {
        "Video cần đăng nhập hoặc là video riêng tư. Vào Cài đặt → Cookie đăng nhập, chọn trình duyệt bạn đã đăng nhập."
    } else if ["resolving timed out", "could not resolve", "getaddrinfo", "timed out", "connection refused"].iter().any(|k| low.contains(k)) {
        "Không kết nối được tới trang web. Kiểm tra mạng (hoặc VPN/proxy) rồi thử lại."
    } else if low.contains("tiktok:user") || low.contains("secondary user id") {
        "yt-dlp hiện chưa tải được cả trang cá nhân TikTok. Hãy copy link từng video (có thể dán nhiều link một lúc)."
    } else if low.contains("unsupported url") {
        "Link này không được hỗ trợ."
    } else if low.contains("ffmpeg") || low.contains("postprocessing") {
        "Lỗi khi ghép/chuyển đổi file. Thử Cài đặt → Cài lại thành phần."
    } else if ["http error 403", "unable to extract", "unable to download", "no video formats"].iter().any(|k| low.contains(k)) {
        "Trang web có thể vừa thay đổi. Thử Cài đặt → Cập nhật yt-dlp rồi tải lại."
    } else {
        ""
    };

    if hint.is_empty() {
        detail
    } else {
        format!("{hint}\n{detail}")
    }
}
