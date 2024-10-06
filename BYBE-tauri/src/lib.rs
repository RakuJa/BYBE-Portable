use bybe::InitializeLogResponsibility;
use std::thread;
use tauri::path::BaseDirectory;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .max_file_size(50_000 /* bytes */)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();
            // Get Environmental Variables
            let env_path = app
                .path()
                .resolve("data/.env", BaseDirectory::Resource)
                .expect("Should find BYBE env variables inside resources")
                .into_os_string()
                .into_string()
                .ok();
            // get DB
            let db_path = app
                .path()
                .resolve("data/database.db", BaseDirectory::Resource)
                .expect("Should find BYBE database inside resources")
                .into_os_string()
                .into_string()
                .ok();
            thread::spawn(move || {
                bybe::start(env_path, db_path, InitializeLogResponsibility::Delegated)
                    .expect("Backend should be able to startup, port or ip busy?");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
