mod deps;
mod download;
mod proxy;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|_| {
            proxy::start();
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(download::Jobs::default())
        .invoke_handler(tauri::generate_handler![
            deps::deps_status,
            deps::install_deps,
            deps::update_ytdlp,
            download::fetch_info,
            download::start_download,
            download::cancel_download,
            download::files_exist,
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
