//! Nén video về một dung lượng đích (ví dụ 27 MB → 20 MB) mà vẫn là file video (.mp4).
//! Hình được nén lại 2 lượt bằng H.264 (lượt 1 phân tích, lượt 2 nén) với bitrate tính từ dung lượng đích, nên file ra
//! sát mục tiêu và đẹp nhất có thể ở dung lượng đó. Âm thanh chép nguyên bản gốc (không nén lại) khi định dạng hợp với
//! mp4. Thử trên video 1080p 68 giây: 27,4 MB → 19,5 MB và 14,7 MB, SSIM 0,989 và 0,986 (gần như y nguyên).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::deps::{self, err};
use crate::i18n::{self, t};

/// Chừa 3% cho phần đầu file mp4 và độ lệch bitrate, để file ra không vượt dung lượng đích.
const SAFETY: f64 = 0.97;
/// Bitrate hình thấp nhất còn xem được; thấp hơn thì báo dung lượng đích quá nhỏ.
const MIN_VIDEO_BPS: f64 = 150_000.0;
/// Âm thanh các định dạng này chép thẳng vào mp4; định dạng khác (vorbis...) thì nén lại sang AAC.
const COPY_AUDIO: [&str; 5] = ["aac", "mp3", "ac3", "eac3", "alac"];

#[derive(Default)]
pub struct Compressions(Mutex<HashMap<String, u32>>);

impl Compressions {
    pub fn kill_all(&self) {
        for pid in self.0.lock().unwrap().values() {
            crate::download::kill_tree(*pid);
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoProbe {
    size: u64,
    duration: f64,
    width: u32,
    height: u32,
    fps: f64,
    audio_codec: Option<String>,
    /// Bit/giây của âm thanh (0 nếu không có tiếng).
    audio_bps: f64,
}

/// Đọc dung lượng, độ dài, độ phân giải và âm thanh của một file video.
#[tauri::command]
pub async fn compress_probe(app: AppHandle, path: String) -> Result<VideoProbe, String> {
    probe(&app, Path::new(&path)).await
}

async fn probe(app: &AppHandle, path: &Path) -> Result<VideoProbe, String> {
    let size = tokio::fs::metadata(path).await.map_err(err)?.len();
    let out = deps::command(app, "ffprobe")?
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .await
        .map_err(err)?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| t(i18n::COMPRESS_NOT_VIDEO).to_string())?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or_else(|| t(i18n::COMPRESS_NOT_VIDEO).to_string())?;
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    let num = |v: &Value| v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v.as_f64());
    let duration = num(&v["format"]["duration"]).filter(|d| *d > 0.0).ok_or_else(|| t(i18n::COMPRESS_NOT_VIDEO).to_string())?;
    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(|r| r.split_once('/'))
        .and_then(|(a, b)| Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok().filter(|b| *b > 0.0)?))
        .filter(|f| *f > 0.0)
        .unwrap_or(30.0);
    Ok(VideoProbe {
        size,
        duration,
        width: video["width"].as_u64().unwrap_or(0) as u32,
        height: video["height"].as_u64().unwrap_or(0) as u32,
        fps,
        audio_codec: audio.and_then(|a| a["codec_name"].as_str()).map(str::to_owned),
        // Không ghi bitrate (mkv/webm) thì ước 160 kbps.
        audio_bps: audio.map(|a| num(&a["bit_rate"]).unwrap_or(160_000.0)).unwrap_or(0.0),
    })
}

/// Tên file ra cạnh file gốc, ví dụ "Video (20MB).mp4"; trùng thì thêm số.
fn output_path(input: &Path, target_mb: f64) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("."));
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let label = format!("{}MB", (target_mb * 10.0).round() / 10.0);
    let mut path = dir.join(format!("{stem} ({label}).mp4"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({label}) {n}.mp4"));
        n += 1;
    }
    path
}

/// Độ cao để mỗi điểm ảnh còn đủ dữ liệu: nén quá mạnh ở 1080p thì hạ xuống 720p/480p cho hình nét hơn là vỡ.
fn target_height(p: &VideoProbe, video_bps: f64) -> Option<u32> {
    const MIN_BITS_PER_PIXEL: f64 = 0.015;
    let bpp = |h: u32| {
        let w = p.width as f64 * h as f64 / p.height.max(1) as f64;
        video_bps / (w * h as f64 * p.fps)
    };
    if p.height == 0 || bpp(p.height) >= MIN_BITS_PER_PIXEL {
        return None;
    }
    [1080, 720, 540, 480, 360].into_iter().filter(|h| *h < p.height).find(|h| bpp(*h) >= MIN_BITS_PER_PIXEL).or(Some(360))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompressProgress {
    id: String,
    percent: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompressDone {
    id: String,
    ok: bool,
    canceled: bool,
    file: Option<String>,
    size: Option<u64>,
    /// Độ cao đã hạ xuống (nếu có), để giao diện báo cho người dùng.
    height: Option<u32>,
    error: Option<String>,
}

/// Bắt đầu nén `input` về khoảng `target_mb` MB (1 MB = 1024 × 1024 byte). Tiến độ qua "compress-progress",
/// kết quả qua "compress-done".
#[tauri::command]
pub async fn compress_video(
    app: AppHandle,
    jobs: State<'_, Compressions>,
    id: String,
    input: String,
    target_mb: f64,
) -> Result<(), String> {
    let input = PathBuf::from(input);
    let p = probe(&app, &input).await?;
    let target_bytes = target_mb * 1024.0 * 1024.0;
    if target_bytes >= p.size as f64 {
        return Err(t(i18n::COMPRESS_NOT_SMALLER).into());
    }
    let copy_audio = p.audio_codec.as_deref().is_some_and(|c| COPY_AUDIO.contains(&c));
    let audio_bps = if p.audio_codec.is_none() {
        0.0
    } else if copy_audio {
        p.audio_bps
    } else {
        p.audio_bps.clamp(96_000.0, 192_000.0)
    };
    let video_bps = target_bytes * 8.0 * SAFETY / p.duration - audio_bps;
    if video_bps < MIN_VIDEO_BPS {
        let min_mb = (MIN_VIDEO_BPS + audio_bps) * p.duration / 8.0 / SAFETY / 1024.0 / 1024.0;
        return Err(t(i18n::COMPRESS_TOO_SMALL).replace("{mb}", &format!("{:.1}", min_mb.ceil())));
    }
    let kbps = (video_bps / 1000.0).floor() as u64;
    let height = target_height(&p, video_bps);
    let output = output_path(&input, target_mb);
    let log_prefix = app.path().app_cache_dir().map_err(err)?.join(format!("compress-{id}"));
    if let Some(dir) = log_prefix.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(err)?;
    }

    // Ghi vào danh sách trước khi chạy: run() coi việc không còn trong danh sách là đã bị huỷ.
    jobs.0.lock().unwrap().insert(id.clone(), 0);
    let app2 = app.clone();
    let done_id = id;
    tauri::async_runtime::spawn(async move {
        let result = run(&app2, &done_id, &input, &output, &log_prefix, kbps, height, copy_audio, audio_bps, p.duration).await;
        let canceled = app2.state::<Compressions>().0.lock().unwrap().remove(&done_id).is_none();
        // Dọn file log của 2 lượt nén.
        for ext in ["-0.log", "-0.log.mbtree"] {
            let _ = tokio::fs::remove_file(format!("{}{ext}", log_prefix.to_string_lossy())).await;
        }
        let done = match result {
            Ok(()) if !canceled => CompressDone {
                id: done_id,
                ok: true,
                canceled: false,
                size: tokio::fs::metadata(&output).await.ok().map(|m| m.len()),
                file: Some(output.to_string_lossy().into_owned()),
                height,
                error: None,
            },
            other => {
                let _ = tokio::fs::remove_file(&output).await;
                CompressDone {
                    id: done_id,
                    ok: false,
                    canceled,
                    file: None,
                    size: None,
                    height: None,
                    error: if canceled { None } else { other.err() },
                }
            }
        };
        let _ = app2.emit("compress-done", done);
    });
    Ok(())
}

/// Chạy 2 lượt ffmpeg; mỗi lượt chiếm một nửa thanh tiến độ.
#[allow(clippy::too_many_arguments)]
async fn run(
    app: &AppHandle,
    id: &str,
    input: &Path,
    output: &Path,
    log_prefix: &Path,
    kbps: u64,
    height: Option<u32>,
    copy_audio: bool,
    audio_bps: f64,
    duration: f64,
) -> Result<(), String> {
    for pass in [1, 2] {
        let mut cmd = deps::command(app, "ffmpeg")?;
        cmd.args(["-hide_banner", "-nostats", "-loglevel", "error", "-progress", "pipe:1", "-y", "-i"])
            .arg(input)
            .args(["-map", "0:v:0", "-c:v", "libx264", "-preset", "medium", "-pix_fmt", "yuv420p"])
            .args(["-b:v", &format!("{kbps}k"), "-pass", &pass.to_string(), "-passlogfile"])
            .arg(log_prefix);
        if let Some(h) = height {
            cmd.args(["-vf", &format!("scale=-2:{h}")]);
        }
        if pass == 1 {
            cmd.args(["-an", "-f", "null", if cfg!(windows) { "NUL" } else { "/dev/null" }]);
        } else {
            cmd.args(["-map", "0:a:0?"]);
            if copy_audio {
                cmd.args(["-c:a", "copy"]);
            } else {
                cmd.args(["-c:a", "aac", "-b:a", &format!("{}k", (audio_bps / 1000.0).round() as u64)]);
            }
            cmd.args(["-movflags", "+faststart"]).arg(output);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(err)?;
        {
            let mut jobs = app.state::<Compressions>().0.lock().unwrap();
            // Đã bị huỷ giữa 2 lượt thì dừng luôn.
            let Some(pid) = jobs.get_mut(id) else {
                let _ = child.start_kill();
                return Err(String::new());
            };
            *pid = child.id().unwrap_or(0);
        }

        let mut stderr = child.stderr.take().unwrap();
        let errors = tokio::spawn(async move {
            let mut buf = String::new();
            let _ = tokio::io::AsyncReadExt::read_to_string(&mut stderr, &mut buf).await;
            buf
        });
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // "-progress" in dòng "out_time_us=12345678" (micro giây đã xử lý).
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                let part = (us / 1e6 / duration).clamp(0.0, 1.0);
                let percent = (if pass == 1 { part } else { 1.0 + part }) * 50.0;
                let _ = app.emit("compress-progress", CompressProgress { id: id.to_owned(), percent });
            }
        }
        let status = child.wait().await.map_err(err)?;
        let log = errors.await.unwrap_or_default();
        if !status.success() {
            let detail = log.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_owned();
            return Err(t(i18n::COMPRESS_FAILED).replace("{e}", &detail));
        }
    }
    Ok(())
}

/// Dừng lượt nén đang chạy (file dở dang sẽ bị xoá).
#[tauri::command]
pub fn compress_cancel(jobs: State<'_, Compressions>, id: String) {
    if let Some(pid) = jobs.0.lock().unwrap().remove(&id) {
        crate::download::kill_tree(pid);
    }
}
