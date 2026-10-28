//! Lấy thông tin video và tải video/MP3 bằng yt-dlp, báo tiến độ lên giao diện qua event.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::deps::{self, err};

const OUTPUT_TEMPLATE: &str = "%(title).150B [%(id)s].%(ext)s";

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
    Mp3,
}

fn mode_args(mode: Mode) -> Vec<&'static str> {
    match mode {
        Mode::Best => vec!["-f", "bv*+ba/b", "--merge-output-format", "mp4", "--remux-video", "mp4"],
        Mode::Compat => vec!["-f", COMPAT_FORMAT, "--merge-output-format", "mp4", "--remux-video", "mp4"],
        Mode::Mp3 => vec!["-f", "ba/b", "-x", "--audio-format", "mp3", "--audio-quality", "320K", "--embed-thumbnail"],
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    title: String,
    thumbnail: Option<String>,
    duration: Option<f64>,
    uploader: Option<String>,
    site: String,
    max_height: Option<u64>,
    url: String,
}

#[tauri::command]
pub async fn fetch_info(app: AppHandle, url: String, cookies: String) -> Result<VideoInfo, String> {
    let out = deps::command(&app, "yt-dlp")?
        .args(["-J", "--no-playlist", "--no-warnings"])
        .args(cookie_args(&cookies))
        .arg("--")
        .arg(&url)
        .output()
        .await
        .map_err(|e| format!("Không chạy được yt-dlp: {e}"))?;
    if !out.status.success() {
        return Err(friendly_error(&String::from_utf8_lossy(&out.stderr)));
    }

    let mut v: Value = serde_json::from_slice(&out.stdout).map_err(err)?;
    if v["_type"] == "playlist" {
        v = v["entries"].get(0).cloned().ok_or_else(|| "Link là danh sách phát nhưng không có video nào.".to_string())?;
    }

    let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let max_height = v["formats"]
        .as_array()
        .and_then(|formats| {
            formats
                .iter()
                .filter(|f| f["vcodec"].as_str() != Some("none"))
                .filter_map(|f| f["height"].as_u64())
                .max()
        })
        .or_else(|| v["height"].as_u64());

    Ok(VideoInfo {
        title: text("title").unwrap_or_else(|| "Không có tiêu đề".into()),
        thumbnail: text("thumbnail"),
        duration: v["duration"].as_f64(),
        uploader: text("uploader").or_else(|| text("channel")),
        site: text("extractor_key").unwrap_or_default(),
        max_height,
        url: text("webpage_url").unwrap_or(url),
    })
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
}

#[derive(Default)]
struct LineState {
    format_id: String,
    part: u32,
    file: Option<String>,
    log: Vec<String>,
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    id: String,
    url: String,
    mode: Mode,
    out_dir: String,
    cookies: String,
) -> Result<(), String> {
    let bin = deps::bin_dir(&app)?;
    let mut cmd = deps::command(&app, "yt-dlp")?;
    cmd.args(["--no-playlist", "--newline", "--progress", "--no-mtime", "--embed-metadata"])
        .arg("--ffmpeg-location")
        .arg(&bin)
        .arg("-P")
        .arg(&out_dir)
        .args(["-o", OUTPUT_TEMPLATE])
        .args(["--progress-template", DOWNLOAD_PROGRESS])
        .args(["--progress-template", POSTPROCESS_PROGRESS])
        .args(["--print", "after_move:FILE|%(filepath)s"])
        .args(cookie_args(&cookies))
        .args(mode_args(mode))
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
        s.log.push(line.to_owned());
        if s.log.len() > 50 {
            s.log.remove(0);
        }
    }
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

    let browser_cookie_error = low.contains("cookie")
        && ["chrome", "edge", "brave", "dpapi", "decrypt", "could not copy"].iter().any(|k| low.contains(k));
    let hint = if browser_cookie_error {
        "Không đọc được cookie từ trình duyệt. Hãy tắt hẳn trình duyệt rồi thử lại, hoặc dùng Firefox / file cookies.txt."
    } else if ["login", "log in", "sign in", "cookies", "private", "registered users", "not a bot"].iter().any(|k| low.contains(k)) {
        "Video cần đăng nhập hoặc là video riêng tư. Vào Cài đặt → Cookie đăng nhập, chọn trình duyệt bạn đã đăng nhập."
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
