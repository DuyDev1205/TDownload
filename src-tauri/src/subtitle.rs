//! Tạo phụ đề cho file trên máy: tách phụ đề có sẵn trong file (ffmpeg), hoặc nhận dạng giọng nói bằng
//! Whisper large-v3-turbo chạy ngay trên máy (whisper.cpp). Bộ nhận dạng (~600 MB) chỉ tải khi dùng lần đầu.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::deps::{self, err, Package, Unpack, EXE};
use crate::download::kill_tree;

/// Whisper large-v3-turbo nén q5_0: thử trên tiếng Việt và tiếng Anh cho phụ đề có dấu câu, viết hoa, số viết
/// bằng chữ số, tốt hơn hẳn PhoWhisper (không dấu câu, đọc số thành chữ) mà còn nhanh hơn.
const MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";

/// Phụ đề dạng hình ảnh (Blu-ray, DVD...) không chuyển thành chữ được.
const BITMAP_CODECS: [&str; 4] = ["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "xsub"];

fn model_package() -> Package {
    Package {
        name: "model",
        url: format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{MODEL}"),
        unpack: Unpack::Data(MODEL),
    }
}

#[cfg(windows)]
fn whisper_packages() -> Vec<Package> {
    vec![
        Package {
            name: "whisper",
            url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-blas-bin-x64.zip".into(),
            unpack: Unpack::ZipWithLibs("whisper-cli"),
        },
        model_package(),
    ]
}

/// whisper.cpp không phát hành bản chạy sẵn cho macOS; bản này do workflow whisper-macos.yml của repo build.
#[cfg(target_os = "macos")]
fn whisper_packages() -> Vec<Package> {
    vec![
        Package {
            name: "whisper",
            url: "https://github.com/DuyDev1205/TDownload/releases/download/whisper-b5454/whisper-cli-macos.zip".into(),
            unpack: Unpack::Zip(&["whisper-cli"]),
        },
        model_package(),
    ]
}

/// whisper-cli và mô hình nằm chung một thư mục riêng; whisper-cli chạy với thư mục này làm thư mục hiện tại và chỉ
/// nhận đường dẫn tương đối, vì trên Windows nó không đọc được đường dẫn có ký tự tiếng Việt.
fn whisper_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(deps::bin_dir(app)?.join("whisper"))
}

#[tauri::command]
pub fn whisper_ready(app: AppHandle) -> Result<bool, String> {
    let dir = whisper_dir(&app)?;
    Ok(whisper_packages().iter().all(|p| p.installed(&dir)))
}

#[tauri::command]
pub async fn install_whisper(app: AppHandle) -> Result<(), String> {
    let dir = whisper_dir(&app)?;
    deps::install_packages(&app, whisper_packages(), &dir, false).await
}

#[derive(Serialize)]
pub struct SubtitleStream {
    index: u64,
    codec: String,
    language: Option<String>,
    title: Option<String>,
    /// Phụ đề dạng chữ, tách ra .srt được.
    text: bool,
}

/// Các luồng phụ đề có sẵn trong file video.
#[tauri::command]
pub async fn probe_subtitles(app: AppHandle, path: String) -> Result<Vec<SubtitleStream>, String> {
    let out = deps::command(&app, "ffprobe")?
        .args(["-v", "error", "-select_streams", "s", "-show_entries", "stream=index,codec_name:stream_tags=language,title"])
        .args(["-of", "json"])
        .arg(&path)
        .output()
        .await
        .map_err(|e| format!("Không chạy được ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(format!("Không đọc được file: {}", last_line(&out.stderr)));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(err)?;
    let tag = |s: &Value, k: &str| s["tags"][k].as_str().filter(|t| !t.is_empty()).map(str::to_owned);
    Ok(v["streams"]
        .as_array()
        .map(|streams| {
            streams
                .iter()
                .map(|s| {
                    let codec = s["codec_name"].as_str().unwrap_or_default().to_string();
                    SubtitleStream {
                        index: s["index"].as_u64().unwrap_or_default(),
                        text: !BITMAP_CODECS.contains(&codec.as_str()),
                        language: tag(s, "language"),
                        title: tag(s, "title"),
                        codec,
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Tách một luồng phụ đề ra file .srt cạnh video, trả về đường dẫn file mới.
#[tauri::command]
pub async fn extract_subtitle(app: AppHandle, path: String, index: u64, language: Option<String>) -> Result<String, String> {
    let suffix = language.filter(|l| !l.is_empty() && l != "und").unwrap_or_else(|| format!("track{index}"));
    let target = unique_srt_path(Path::new(&path), &suffix);
    let out = deps::command(&app, "ffmpeg")?
        .args(["-v", "error", "-y", "-i"])
        .arg(&path)
        .args(["-map", &format!("0:{index}"), "-c:s", "srt"])
        .arg(&target)
        .output()
        .await
        .map_err(|e| format!("Không chạy được ffmpeg: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&target);
        return Err(format!("Không tách được phụ đề: {}", last_line(&out.stderr)));
    }
    Ok(target.to_string_lossy().into_owned())
}

/// `video.mp4` thành `video.<suffix>.srt`; trùng tên thì thêm (1), (2)...
fn unique_srt_path(src: &Path, suffix: &str) -> PathBuf {
    let dir = src.parent().unwrap_or(Path::new("."));
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("phu-de");
    let mut target = dir.join(format!("{stem}.{suffix}.srt"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("{stem}.{suffix} ({n}).srt"));
        n += 1;
    }
    target
}

fn last_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("lỗi không rõ").trim().to_string()
}

struct Job {
    pid: u32,
    canceled: bool,
}

#[derive(Default)]
pub struct Transcribes(Mutex<HashMap<String, Job>>);

impl Transcribes {
    pub fn kill_all(&self) {
        for job in self.0.lock().unwrap().values() {
            kill_tree(job.pid);
        }
    }

    fn set_pid(&self, id: &str, pid: u32) {
        if let Some(job) = self.0.lock().unwrap().get_mut(id) {
            job.pid = pid;
        }
    }

    fn canceled(&self, id: &str) -> bool {
        self.0.lock().unwrap().get(id).is_some_and(|j| j.canceled)
    }
}

#[derive(Clone, Serialize)]
struct TrProgress {
    id: String,
    /// "convert" (tách âm thanh) hoặc "transcribe" (nhận dạng).
    stage: &'static str,
    percent: Option<f64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrDone {
    id: String,
    ok: bool,
    canceled: bool,
    file: Option<String>,
    error: Option<String>,
    /// Ngôn ngữ Whisper nhận ra (khi chọn "Tự nhận").
    language: Option<String>,
}

/// Nhận dạng giọng nói trong file và lưu phụ đề .srt cạnh file. `language` là "auto", "vi", "en"...
/// Chạy nền; tiến độ báo qua event "tr-progress", kết quả qua "tr-done".
#[tauri::command]
pub fn transcribe(app: AppHandle, jobs: State<'_, Transcribes>, id: String, path: String, language: String) -> Result<(), String> {
    let dir = whisper_dir(&app)?;
    if !whisper_packages().iter().all(|p| p.installed(&dir)) {
        return Err("Chưa cài bộ nhận dạng giọng nói.".into());
    }
    jobs.0.lock().unwrap().insert(id.clone(), Job { pid: 0, canceled: false });

    tauri::async_runtime::spawn(async move {
        let result = run_transcribe(&app, &dir, &id, &path, &language).await;
        let _ = std::fs::remove_file(dir.join("work").join(format!("{id}.wav")));
        let _ = std::fs::remove_file(dir.join("work").join(format!("{id}.srt")));
        let canceled = app.state::<Transcribes>().0.lock().unwrap().remove(&id).is_some_and(|j| j.canceled);
        let done = match result {
            Ok((file, detected)) if !canceled => {
                TrDone { id, ok: true, canceled, file: Some(file), error: None, language: detected }
            }
            Ok(_) => TrDone { id, ok: false, canceled, file: None, error: None, language: None },
            Err(e) => TrDone { id, ok: false, canceled, file: None, error: (!canceled).then_some(e), language: None },
        };
        let _ = app.emit("tr-done", done);
    });
    Ok(())
}

async fn run_transcribe(app: &AppHandle, dir: &Path, id: &str, path: &str, language: &str) -> Result<(String, Option<String>), String> {
    let jobs = app.state::<Transcribes>();
    let work = dir.join("work");
    std::fs::create_dir_all(&work).map_err(err)?;
    let emit = |stage: &'static str, percent: Option<f64>| {
        let _ = app.emit("tr-progress", TrProgress { id: id.to_string(), stage, percent });
    };

    // 1. Tách âm thanh thành WAV 16 kHz mono, đúng định dạng Whisper cần.
    emit("convert", None);
    let mut ffmpeg = deps::command(app, "ffmpeg")?
        .args(["-v", "error", "-y", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(work.join(format!("{id}.wav")))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Không chạy được ffmpeg: {e}"))?;
    jobs.set_pid(id, ffmpeg.id().unwrap_or(0));
    let out = ffmpeg.wait_with_output().await.map_err(err)?;
    if jobs.canceled(id) {
        return Err("canceled".into());
    }
    if !out.status.success() {
        return Err(format!("Không tách được âm thanh (file có tiếng không?): {}", last_line(&out.stderr)));
    }

    // 2. Nhận dạng. Chỉ dùng đường dẫn tương đối tính từ thư mục whisper (xem whisper_dir).
    emit("transcribe", Some(0.0));
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4).to_string();
    let mut cmd = tokio::process::Command::new(dir.join(format!("whisper-cli{EXE}")));
    cmd.current_dir(dir)
        .args(["-m", MODEL, "-l", language, "-t", &threads, "-osrt", "-pp"])
        .args(["-f", &format!("work/{id}.wav"), "-of", &format!("work/{id}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(deps::CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| format!("Không chạy được bộ nhận dạng: {e}"))?;
    jobs.set_pid(id, child.id().unwrap_or(0));

    // whisper-cli in tiến độ ra stderr: "...progress =  41%" và "auto-detected language: vi (p = 0.99)".
    let mut detected = None;
    let mut log = Vec::new();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(p) = line.split("progress =").nth(1) {
            if let Ok(p) = p.trim().trim_end_matches('%').trim().parse::<f64>() {
                emit("transcribe", Some(p));
            }
        } else if let Some(rest) = line.split("auto-detected language:").nth(1) {
            detected = rest.split_whitespace().next().map(str::to_owned);
        } else if !line.trim().is_empty() {
            log.push(line);
            if log.len() > 30 {
                log.remove(0);
            }
        }
    }
    let status = child.wait().await.map_err(err)?;
    if jobs.canceled(id) {
        return Err("canceled".into());
    }
    let srt = work.join(format!("{id}.srt"));
    if !status.success() || !srt.exists() {
        return Err(format!("Nhận dạng lỗi: {}", log.last().map(String::as_str).unwrap_or("không rõ nguyên nhân")));
    }

    // 3. Ghi phụ đề cạnh file gốc. whisper-cli để một dấu cách ở đầu mỗi câu: bỏ đi.
    let text = std::fs::read_to_string(&srt).map_err(err)?;
    let cleaned: String = text.lines().map(|l| format!("{}\n", l.trim_start())).collect();
    let suffix = detected.clone().unwrap_or_else(|| language.to_string());
    let target = unique_srt_path(Path::new(path), if suffix == "auto" { "ai" } else { suffix.as_str() });
    std::fs::write(&target, cleaned).map_err(|e| format!("Không ghi được file phụ đề: {e}"))?;
    Ok((target.to_string_lossy().into_owned(), detected))
}

#[tauri::command]
pub fn cancel_transcribe(jobs: State<'_, Transcribes>, id: String) {
    let pid = match jobs.0.lock().unwrap().get_mut(&id) {
        Some(job) => {
            job.canceled = true;
            job.pid
        }
        None => return,
    };
    std::thread::spawn(move || kill_tree(pid));
}
