//! Quản lý các công cụ ngoài: yt-dlp (lấy link + tải), ffmpeg/ffprobe (ghép hình + tiếng, chuyển MP3)
//! và deno (yt-dlp cần để giải mã YouTube). Tất cả được tải về thư mục dữ liệu của app ở lần chạy đầu,
//! nhờ vậy bộ cài rất nhẹ và yt-dlp có thể tự cập nhật mà không cần quyền admin.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

#[cfg(windows)]
pub const EXE: &str = ".exe";
#[cfg(not(windows))]
pub const EXE: &str = "";

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const TOOLS: [&str; 4] = ["yt-dlp", "ffmpeg", "ffprobe", "deno"];

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

enum Unpack {
    /// File tải về chính là file chạy, lưu với tên này.
    Raw(&'static str),
    /// File zip, lấy ra các file chạy có tên này (ở bất kỳ thư mục con nào).
    Zip(&'static [&'static str]),
}

struct Package {
    name: &'static str,
    url: String,
    unpack: Unpack,
}

impl Package {
    fn files(&self) -> Vec<&'static str> {
        match self.unpack {
            Unpack::Raw(name) => vec![name],
            Unpack::Zip(names) => names.to_vec(),
        }
    }

    fn installed(&self, dir: &Path) -> bool {
        self.files().iter().all(|f| dir.join(format!("{f}{EXE}")).exists())
    }
}

#[cfg(windows)]
fn packages() -> Vec<Package> {
    vec![
        Package {
            name: "yt-dlp",
            url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe".into(),
            unpack: Unpack::Raw("yt-dlp"),
        },
        Package {
            name: "ffmpeg",
            url: "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip".into(),
            unpack: Unpack::Zip(&["ffmpeg", "ffprobe"]),
        },
        Package {
            name: "deno",
            url: "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip".into(),
            unpack: Unpack::Zip(&["deno"]),
        },
    ]
}

#[cfg(target_os = "macos")]
fn packages() -> Vec<Package> {
    let (ff_arch, deno_arch) = if cfg!(target_arch = "aarch64") {
        ("arm64", "aarch64")
    } else {
        ("amd64", "x86_64")
    };
    vec![
        Package {
            name: "yt-dlp",
            url: "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos".into(),
            unpack: Unpack::Raw("yt-dlp"),
        },
        Package {
            name: "ffmpeg",
            url: format!("https://ffmpeg.martin-riedl.de/redirect/latest/macos/{ff_arch}/release/ffmpeg.zip"),
            unpack: Unpack::Zip(&["ffmpeg"]),
        },
        Package {
            name: "ffprobe",
            url: format!("https://ffmpeg.martin-riedl.de/redirect/latest/macos/{ff_arch}/release/ffprobe.zip"),
            unpack: Unpack::Zip(&["ffprobe"]),
        },
        Package {
            name: "deno",
            url: format!("https://github.com/denoland/deno/releases/latest/download/deno-{deno_arch}-apple-darwin.zip"),
            unpack: Unpack::Zip(&["deno"]),
        },
    ]
}

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("VidSaver hiện chỉ hỗ trợ Windows và macOS.");

pub fn bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(err)?.join("bin");
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

/// Tạo lệnh chạy một công cụ trong thư mục bin: không hiện cửa sổ console,
/// và đưa thư mục bin lên đầu PATH để yt-dlp tự tìm thấy ffmpeg và deno.
pub fn command(app: &AppHandle, tool: &str) -> Result<tokio::process::Command, String> {
    let dir = bin_dir(app)?;
    let mut paths = vec![dir.clone()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }

    let mut cmd = tokio::process::Command::new(dir.join(format!("{tool}{EXE}")));
    cmd.env("PATH", std::env::join_paths(paths).map_err(err)?)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null());
    // yt-dlp.exe đóng gói bằng PyInstaller nên bỏ qua các biến PYTHON* ở trên và in theo bảng mã của Windows
    // (cp1252), làm hỏng đường dẫn có tiếng Việt. Phải ép UTF-8 bằng tham số của chính yt-dlp.
    if tool == "yt-dlp" {
        cmd.args(["--encoding", "utf-8"]);
    }
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    Ok(cmd)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepsStatus {
    ready: bool,
    missing: Vec<String>,
    ytdlp_version: Option<String>,
    bin_dir: String,
}

#[tauri::command]
pub async fn deps_status(app: AppHandle) -> Result<DepsStatus, String> {
    let dir = bin_dir(&app)?;
    let missing: Vec<String> = TOOLS
        .iter()
        .filter(|t| !dir.join(format!("{t}{EXE}")).exists())
        .map(|t| t.to_string())
        .collect();

    let mut ytdlp_version = None;
    if !missing.iter().any(|m| m == "yt-dlp") {
        if let Ok(out) = command(&app, "yt-dlp")?.arg("--version").output().await {
            if out.status.success() {
                ytdlp_version = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
            }
        }
    }

    Ok(DepsStatus {
        ready: missing.is_empty(),
        missing,
        ytdlp_version,
        bin_dir: dir.to_string_lossy().into_owned(),
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DepsProgress {
    name: &'static str,
    stage: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

fn emit_progress(app: &AppHandle, name: &'static str, stage: &'static str, downloaded: u64, total: Option<u64>) {
    let _ = app.emit("deps-progress", DepsProgress { name, stage, downloaded, total });
}

#[tauri::command]
pub async fn install_deps(app: AppHandle, force: bool) -> Result<(), String> {
    let dir = bin_dir(&app)?;
    let client = reqwest::Client::builder().user_agent("VidSaver").build().map_err(err)?;

    for pkg in packages() {
        if !force && pkg.installed(&dir) {
            emit_progress(&app, pkg.name, "done", 0, None);
            continue;
        }

        let tmp = dir.join(format!("{}.download", pkg.name));
        let result = install_one(&app, &client, &pkg, &dir, &tmp).await;
        let _ = std::fs::remove_file(&tmp);
        result?;
        emit_progress(&app, pkg.name, "done", 0, None);
    }
    Ok(())
}

async fn install_one(app: &AppHandle, client: &reqwest::Client, pkg: &Package, dir: &Path, tmp: &Path) -> Result<(), String> {
    download_file(app, client, pkg, tmp).await?;
    emit_progress(app, pkg.name, "extract", 0, None);

    match pkg.unpack {
        Unpack::Raw(name) => {
            let target = dir.join(format!("{name}{EXE}"));
            let _ = std::fs::remove_file(&target);
            std::fs::rename(tmp, &target).map_err(err)?;
            make_executable(&target)
        }
        Unpack::Zip(names) => {
            let (zip_path, dir) = (tmp.to_path_buf(), dir.to_path_buf());
            tauri::async_runtime::spawn_blocking(move || extract(&zip_path, &dir, names))
                .await
                .map_err(err)?
        }
    }
}

async fn download_file(app: &AppHandle, client: &reqwest::Client, pkg: &Package, dest: &Path) -> Result<(), String> {
    let resp = client
        .get(&pkg.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Không tải được {}: {e}", pkg.name))?;
    let total = resp.content_length();

    let mut file = tokio::fs::File::create(dest).await.map_err(err)?;
    let mut stream = resp.bytes_stream();
    let (mut done, mut last_emit) = (0u64, 0u64);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Mất kết nối khi tải {}: {e}", pkg.name))?;
        file.write_all(&chunk).await.map_err(err)?;
        done += chunk.len() as u64;
        if done - last_emit >= 512 * 1024 {
            last_emit = done;
            emit_progress(app, pkg.name, "download", done, total);
        }
    }
    file.flush().await.map_err(err)?;
    emit_progress(app, pkg.name, "download", done, total);
    Ok(())
}

fn extract(zip_path: &Path, dir: &Path, wanted: &[&str]) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(err)?).map_err(err)?;
    let mut found = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(err)?;
        if entry.is_dir() {
            continue;
        }
        let Some(base) = Path::new(entry.name()).file_name().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        if !wanted.iter().any(|w| base == format!("{w}{EXE}")) {
            continue;
        }
        let target = dir.join(&base);
        let mut out = std::fs::File::create(&target).map_err(err)?;
        std::io::copy(&mut entry, &mut out).map_err(err)?;
        make_executable(&target)?;
        found += 1;
    }
    if found < wanted.len() {
        return Err(format!("Gói tải về thiếu file: {}", wanted.join(", ")));
    }
    Ok(())
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(err)?;
    }
    let _ = path;
    Ok(())
}

/// Chạy `yt-dlp -U`. Các trang thay đổi thường xuyên nên cần cập nhật định kỳ.
#[tauri::command]
pub async fn update_ytdlp(app: AppHandle) -> Result<String, String> {
    let out = command(&app, "yt-dlp")?.arg("-U").output().await.map_err(err)?;
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let last = text.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("").to_string();
    if out.status.success() {
        Ok(last)
    } else {
        Err(last)
    }
}
