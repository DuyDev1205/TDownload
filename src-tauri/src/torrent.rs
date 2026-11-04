//! Tải torrent (link magnet, link hoặc file .torrent do người dùng tự đưa vào) bằng thư viện librqbit nhúng trong app.
//! Phiên tải lưu trạng thái trong thư mục dữ liệu của app, nên tắt app rồi mở lại vẫn tải tiếp. Phiên chỉ khởi động
//! khi người dùng mở torrent lần đầu (hoặc lúc mở app nếu đã từng dùng), để ai không dùng torrent thì app không mở
//! kết nối nào. Tải xong thì dừng chia sẻ cho đỡ tốn mạng.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStatsState,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::OnceCell;

use crate::i18n::{self, t};

/// Thời gian chờ tối đa để lấy thông tin (danh sách file) của một link magnet.
const METADATA_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct Torrents(OnceCell<Arc<Session>>);

pub fn session_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("torrent-session"))
}

/// Phiên tải torrent, khởi động ở lần gọi đầu tiên.
pub async fn session(app: &AppHandle) -> Result<Arc<Session>, String> {
    let state = app.state::<Torrents>();
    let session = state
        .0
        .get_or_try_init(|| async {
            let dir = session_dir(app)?;
            tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
            let opts = SessionOptions {
                fastresume: true,
                persistence: Some(SessionPersistenceConfig::Json { folder: Some(dir) }),
                ..Default::default()
            };
            Session::new_with_opts(default_folder(app), opts)
                .await
                .map_err(|e| t(i18n::TORRENT_START_FAILED).replace("{e}", &format!("{e:#}")))
        })
        .await?;
    Ok(session.clone())
}

/// Thư mục mặc định của phiên. librqbit đặt torrent nhiều file vào một thư mục con trong đó; app tính thư mục con này
/// rồi đặt nó trong thư mục lưu người dùng chọn.
fn default_folder(app: &AppHandle) -> PathBuf {
    app.path().download_dir().unwrap_or_else(|_| std::env::temp_dir())
}

fn find(session: &Session, id: usize) -> Result<Arc<ManagedTorrent>, String> {
    session.get(TorrentIdOrHash::Id(id)).ok_or_else(|| t(i18n::TORRENT_NOT_FOUND).to_string())
}

fn anyhow_msg(text: [&'static str; 6], e: anyhow::Error) -> String {
    t(text).replace("{e}", &format!("{e:#}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    index: usize,
    name: String,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentAdded {
    id: usize,
    name: String,
    folder: String,
    files: Vec<TorrentFile>,
    /// Torrent này đã có trong danh sách từ trước.
    already: bool,
}

/// Mở một torrent ở trạng thái tạm dừng và trả về danh sách file để người dùng chọn, rồi mới bắt đầu tải (torrent_start).
/// `source`: link magnet, mã hash, link http(s) tới file .torrent, hoặc đường dẫn file .torrent trên máy.
#[tauri::command]
pub async fn torrent_add(app: AppHandle, source: String, out_dir: String) -> Result<TorrentAdded, String> {
    let session = session(&app).await?;
    let source = source.trim().to_string();
    let add = if Path::new(&source).is_file() {
        let bytes = tokio::fs::read(&source)
            .await
            .map_err(|e| t(i18n::TORRENT_READ_FILE).replace("{e}", &e.to_string()))?;
        AddTorrent::from_bytes(bytes)
    } else {
        AddTorrent::from_url(source)
    };

    // Bước 1: chỉ đọc thông tin (với magnet là hỏi các máy đang chia sẻ) để biết tên thư mục con của torrent.
    let listed = tokio::time::timeout(
        METADATA_TIMEOUT,
        session.add_torrent(add, Some(AddTorrentOptions { list_only: true, ..Default::default() })),
    )
    .await
    .map_err(|_| t(i18n::TORRENT_NO_METADATA).to_string())?
    .map_err(|e| anyhow_msg(i18n::TORRENT_ADD_FAILED, e))?;

    let (id, handle, already) = match listed {
        AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle, true),
        AddTorrentResponse::Added(id, handle) => (id, handle, false),
        AddTorrentResponse::ListOnly(list) => {
            // Torrent nhiều file thì librqbit đặt vào thư mục con mang tên torrent: giữ thư mục con đó nhưng đặt
            // trong thư mục lưu người dùng đã chọn.
            let base = default_folder(&app);
            let sub = list.output_folder.strip_prefix(&base).unwrap_or(Path::new(""));
            let output_folder = Path::new(&out_dir).join(sub);
            let opts = AddTorrentOptions {
                paused: true,
                overwrite: true,
                output_folder: Some(output_folder.to_string_lossy().into_owned()),
                initial_peers: Some(list.seen_peers),
                ..Default::default()
            };
            match session
                .add_torrent(AddTorrent::from_bytes(list.torrent_bytes), Some(opts))
                .await
                .map_err(|e| anyhow_msg(i18n::TORRENT_ADD_FAILED, e))?
            {
                AddTorrentResponse::Added(id, handle) => (id, handle, false),
                AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle, true),
                AddTorrentResponse::ListOnly(_) => return Err(t(i18n::UNKNOWN_ERROR).into()),
            }
        }
    };

    let files = handle
        .with_metadata(|m| {
            m.file_infos
                .iter()
                .enumerate()
                .filter(|(_, f)| !f.attrs.padding)
                .map(|(index, f)| TorrentFile {
                    index,
                    name: f.relative_filename.to_string_lossy().into_owned(),
                    size: f.len,
                })
                .collect::<Vec<_>>()
        })
        .map_err(|e| anyhow_msg(i18n::TORRENT_ADD_FAILED, e))?;
    Ok(TorrentAdded {
        id,
        name: handle.name().unwrap_or_default(),
        folder: handle.output_folder().to_string_lossy().into_owned(),
        files,
        already,
    })
}

/// Bắt đầu tải các file đã chọn của torrent vừa mở.
#[tauri::command]
pub async fn torrent_start(app: AppHandle, id: usize, files: Vec<usize>) -> Result<(), String> {
    let session = session(&app).await?;
    let handle = find(&session, id)?;
    let only: HashSet<usize> = files.into_iter().collect();
    session.update_only_files(&handle, &only).await.map_err(|e| anyhow_msg(i18n::TORRENT_ADD_FAILED, e))?;
    session.unpause(&handle).await.map_err(|e| anyhow_msg(i18n::TORRENT_ADD_FAILED, e))
}

#[tauri::command]
pub async fn torrent_pause(app: AppHandle, id: usize) -> Result<(), String> {
    let session = session(&app).await?;
    let handle = find(&session, id)?;
    session.pause(&handle).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn torrent_resume(app: AppHandle, id: usize) -> Result<(), String> {
    let session = session(&app).await?;
    let handle = find(&session, id)?;
    session.unpause(&handle).await.map_err(|e| format!("{e:#}"))
}

/// Bỏ torrent khỏi danh sách; `delete_files` thì xoá luôn các file đã tải.
#[tauri::command]
pub async fn torrent_remove(app: AppHandle, id: usize, delete_files: bool) -> Result<(), String> {
    let session = session(&app).await?;
    session.delete(TorrentIdOrHash::Id(id), delete_files).await.map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentStatus {
    id: usize,
    name: String,
    /// "checking" | "downloading" | "paused" | "done" | "error"
    state: &'static str,
    progress: u64,
    total: u64,
    /// Byte/giây.
    speed: u64,
    error: Option<String>,
    folder: String,
}

/// Trạng thái mọi torrent. Chưa từng dùng torrent thì trả danh sách rỗng mà không khởi động phiên tải.
#[tauri::command]
pub async fn torrent_list(app: AppHandle) -> Result<Vec<TorrentStatus>, String> {
    let Some(session) = app.state::<Torrents>().0.get().cloned() else {
        return Ok(Vec::new());
    };
    let handles: Vec<(usize, Arc<ManagedTorrent>)> =
        session.with_torrents(|it| it.map(|(id, h)| (id, h.clone())).collect());
    let mut out = Vec::with_capacity(handles.len());
    for (id, handle) in handles {
        let stats = handle.stats();
        // Tải xong thì dừng chia sẻ (seeding) để đỡ tốn mạng.
        if stats.finished && matches!(stats.state, TorrentStatsState::Live) {
            let _ = session.pause(&handle).await;
        }
        let state = match stats.state {
            _ if stats.error.is_some() => "error",
            TorrentStatsState::Error => "error",
            TorrentStatsState::Initializing { .. } => "checking",
            _ if stats.finished => "done",
            TorrentStatsState::Paused => "paused",
            TorrentStatsState::Live => "downloading",
        };
        out.push(TorrentStatus {
            id,
            name: handle.name().unwrap_or_default(),
            state,
            progress: stats.progress_bytes,
            total: stats.total_bytes,
            speed: stats.live.as_ref().map(|l| l.download_speed.as_bytes()).unwrap_or(0),
            error: stats.error.clone(),
            folder: handle.output_folder().to_string_lossy().into_owned(),
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}
