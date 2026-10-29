//! Gửi góp ý / báo lỗi và quản trị chúng qua máy chủ Google Apps Script (xem feedback/Code.gs).
//! Gọi từ Rust thay vì từ giao diện để không vướng CORS và CSP của webview.

use std::time::Duration;

use serde_json::Value;

use crate::deps::err;

/// Gửi một yêu cầu tới máy chủ góp ý, trả về phần `data` của kết quả.
/// Chỉ cho phép địa chỉ Google Apps Script để lệnh này không bị dùng gọi tới nơi khác.
#[tauri::command]
pub async fn feedback_request(url: String, body: Value) -> Result<Value, String> {
    if !url.starts_with("https://script.google.com/macros/s/") {
        return Err("Địa chỉ máy chủ góp ý không hợp lệ.".into());
    }
    let client = reqwest::Client::builder()
        .user_agent("VidSaver")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(err)?;
    // Apps Script trả 302 sang script.googleusercontent.com; reqwest tự đi theo bằng GET như trình duyệt.
    let text = client
        .post(&url)
        .header("Content-Type", "text/plain;charset=utf-8")
        .body(body.to_string())
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Không kết nối được máy chủ góp ý: {e}"))?
        .text()
        .await
        .map_err(err)?;
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| "Máy chủ góp ý trả về dữ liệu lạ (đã triển khai Apps Script đúng cách chưa?).".to_string())?;
    if v["ok"].as_bool() == Some(true) {
        Ok(v["data"].clone())
    } else {
        Err(v["error"].as_str().unwrap_or("Máy chủ góp ý báo lỗi.").to_string())
    }
}
