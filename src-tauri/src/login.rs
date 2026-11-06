//! Đăng nhập ngay trong app: mở trang web trong một cửa sổ trình duyệt (WebView) để người dùng tự đăng nhập, rồi đọc
//! cookie của WebView ghi ra file cookies.txt cho yt-dlp. Không còn lỗi đọc cookie của Chrome/Edge, và người dùng
//! không phải tự xuất cookie. Mọi cửa sổ của app dùng chung kho cookie nên đăng nhập một lần là nhớ cho lần sau.
//! Cửa sổ đăng nhập mở trang bên ngoài nên không có quyền gọi lệnh nào của app (chỉ cửa sổ "main" có capability).

use std::fmt::Write as _;
use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::i18n::{self, t};

const LABEL: &str = "login";

/// Giá trị cookie đặc biệt từ giao diện: dùng cookie của cửa sổ đăng nhập trong app.
pub const APP_COOKIES: &str = "app";

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main").ok_or_else(|| t(i18n::UNKNOWN_ERROR).to_string())
}

fn cookie_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("login-cookies.txt"))
}

/// Mở (hoặc chuyển) cửa sổ đăng nhập tới `url`. Đóng cửa sổ thì báo "login-closed" để giao diện tự thử lại.
#[tauri::command]
pub async fn login_open(app: AppHandle, url: String) -> Result<(), String> {
    let url: Url = url.trim().parse().map_err(|_| t(i18n::LOGIN_BAD_URL).to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(t(i18n::LOGIN_BAD_URL).into());
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        window.navigate(url).map_err(|e| e.to_string())?;
        let _ = window.set_focus();
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url))
        .title(t(i18n::LOGIN_TITLE))
        .inner_size(1000.0, 760.0)
        .build()
        .map_err(|e| e.to_string())?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            let _ = handle.emit("login-closed", ());
        }
    });
    Ok(())
}

/// Ghi mọi cookie của WebView ra file theo định dạng Netscape (định dạng yt-dlp đọc), trả về đường dẫn file.
async fn export(app: &AppHandle) -> Result<PathBuf, String> {
    let cookies = main_window(app)?.cookies().map_err(|e| e.to_string())?;
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in &cookies {
        let Some(domain) = c.domain().filter(|d| !d.is_empty()) else { continue };
        let http_only = if c.http_only().unwrap_or(false) { "#HttpOnly_" } else { "" };
        let subdomains = if domain.starts_with('.') { "TRUE" } else { "FALSE" };
        let secure = if c.secure().unwrap_or(false) { "TRUE" } else { "FALSE" };
        // Cookie phiên (không có hạn) ghi 0: yt-dlp vẫn dùng.
        let expires = c.expires_datetime().map(|d| d.unix_timestamp().max(0)).unwrap_or(0);
        let path = c.path().unwrap_or("/");
        let _ = writeln!(out, "{http_only}{domain}\t{subdomains}\t{path}\t{secure}\t{expires}\t{}\t{}", c.name(), c.value());
    }
    let file = cookie_file(app)?;
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&file, out).await.map_err(|e| e.to_string())?;
    Ok(file)
}

/// Đổi lựa chọn cookie của giao diện thành tham số cho yt-dlp: "app" thì xuất cookie của cửa sổ đăng nhập ra file.
pub async fn resolve(app: &AppHandle, cookies: String) -> Result<String, String> {
    if cookies != APP_COOKIES {
        return Ok(cookies);
    }
    Ok(format!("file:{}", export(app).await?.to_string_lossy()))
}

/// Đăng xuất mọi trang đã đăng nhập trong app: xoá cookie của các trang web (không đụng tới cài đặt, lịch sử của app).
#[tauri::command]
pub async fn login_clear(app: AppHandle) -> Result<usize, String> {
    let window = main_window(&app)?;
    let cookies = window.cookies().map_err(|e| e.to_string())?;
    let count = cookies.len();
    for c in cookies {
        let _ = window.delete_cookie(c);
    }
    if let Ok(file) = cookie_file(&app) {
        let _ = tokio::fs::remove_file(file).await;
    }
    Ok(count)
}
