//! Proxy HTTP nội bộ ở 127.0.0.1 để yt-dlp đi qua.
//!
//! curl_cffi (yt-dlp dùng để giả lập trình duyệt, bắt buộc với TikTok) tự hỏi DNS và bị treo 20 giây rồi báo
//! "Resolving timed out" trên các máy có card mạng ảo (Hyper-V, WSL, VPN, ...) khai báo DNS không trả lời.
//! Đi qua proxy thì curl gửi nguyên tên miền cho proxy, còn proxy phân giải bằng DNS của hệ điều hành như
//! trình duyệt. HTTPS chỉ được chuyển tiếp nguyên vẹn (CONNECT) nên dấu vân tay TLS giả lập vẫn giữ nguyên.

use std::sync::OnceLock;

use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

static PORT: OnceLock<u16> = OnceLock::new();

/// Mở proxy ở một cổng trống. Không mở nếu người dùng đã tự đặt proxy qua biến môi trường.
pub fn start() {
    if ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"]
        .iter()
        .any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
    {
        return;
    }
    let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", 0)) else { return };
    let Ok(addr) = listener.local_addr() else { return };
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let _ = PORT.set(addr.port());

    tauri::async_runtime::spawn(async move {
        let Ok(listener) = TcpListener::from_std(listener) else { return };
        loop {
            if let Ok((client, _)) = listener.accept().await {
                tauri::async_runtime::spawn(async move {
                    let _ = handle(client).await;
                });
            }
        }
    });
}

/// Tham số cho yt-dlp để đi qua proxy (rỗng nếu proxy không chạy).
pub fn args() -> Vec<String> {
    PORT.get().map(|p| vec!["--proxy".into(), format!("http://127.0.0.1:{p}")]).unwrap_or_default()
}

async fn handle(mut client: TcpStream) -> std::io::Result<()> {
    // Đọc phần đầu request, tới dòng trống.
    let mut buf = Vec::with_capacity(4096);
    let head_end = loop {
        let mut chunk = [0u8; 4096];
        let n = client.read(&mut chunk).await?;
        if n == 0 || buf.len() > 64 * 1024 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n").filter(|l| !l.is_empty());
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let (method, target, version) = (
        request_line.next().unwrap_or_default(),
        request_line.next().unwrap_or_default(),
        request_line.next().unwrap_or("HTTP/1.1"),
    );

    // HTTPS: "CONNECT host:443" → mở đường hầm rồi chuyển tiếp nguyên dữ liệu đã mã hoá.
    if method.eq_ignore_ascii_case("CONNECT") {
        let Ok(mut upstream) = TcpStream::connect(target).await else {
            return client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
        };
        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
        upstream.write_all(&buf[head_end..]).await?;
        copy_bidirectional(&mut client, &mut upstream).await?;
        return Ok(());
    }

    // HTTP thường: "GET http://host[:port]/path" → gửi lại cho host với đường dẫn tương đối.
    let Some(rest) = target.strip_prefix("http://") else {
        return client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let has_port = !authority.ends_with(']') && authority.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok());
    let addr = if has_port { authority.to_string() } else { format!("{authority}:80") };
    let Ok(mut upstream) = TcpStream::connect(addr).await else {
        return client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
    };

    // Mỗi kết nối chỉ phục vụ một request để không lẫn host khi client dùng lại kết nối.
    let mut out = format!("{method} {path} {version}\r\n");
    for header in lines {
        let name = header.split(':').next().unwrap_or_default().trim();
        if ["connection", "proxy-connection", "proxy-authorization"].iter().any(|h| name.eq_ignore_ascii_case(h)) {
            continue;
        }
        out.push_str(header);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    upstream.write_all(out.as_bytes()).await?;
    upstream.write_all(&buf[head_end..]).await?;
    copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}
