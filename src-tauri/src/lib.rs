mod deps;
mod download;
mod feedback;
mod i18n;
mod proxy;
mod torrent;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            proxy::start();
            // Bản 0.5.0 có tính năng tạo phụ đề (đã gỡ) tải bộ nhận dạng ~600 MB vào bin/whisper: xoá cho đỡ chiếm chỗ.
            if let Ok(dir) = deps::bin_dir(app.handle()) {
                std::thread::spawn(move || {
                    let _ = std::fs::remove_dir_all(dir.join("whisper"));
                });
            }
            // Đã từng dùng torrent thì khởi động phiên tải ngay để các torrent dở dang tải tiếp.
            if torrent::session_dir(app.handle()).is_ok_and(|d| d.exists()) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = torrent::session(&handle).await;
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(download::Jobs::default())
        .manage(torrent::Torrents::default())
        .invoke_handler(tauri::generate_handler![
            deps::deps_status,
            deps::install_deps,
            deps::update_ytdlp,
            download::fetch_info,
            download::start_download,
            download::cancel_download,
            download::files_exist,
            feedback::feedback_request,
            i18n::set_language,
            torrent::torrent_add,
            torrent::torrent_start,
            torrent::torrent_pause,
            torrent::torrent_resume,
            torrent::torrent_remove,
            torrent::torrent_list,
        ])
        .build(tauri::generate_context!())
        .expect("không khởi động được ứng dụng")
        .run(|app, event| {
            // Đóng app thì dừng luôn các lượt tải đang chạy.
            if let tauri::RunEvent::Exit = event {
                app.state::<download::Jobs>().kill_all();
            }
        });
}
