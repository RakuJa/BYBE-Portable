use bybe::InitializeLogResponsibility;
use std::thread;
use tauri::path::BaseDirectory;
use tauri::{App, Manager};

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
            let db_path = get_db_path(app).ok();
            thread::spawn(move || {
                bybe::start(env_path, db_path, InitializeLogResponsibility::Delegated)
                    .expect("Backend should be able to startup, port or ip busy?");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "windows")]
pub fn get_db_path(app: &mut App) -> anyhow::Result<String> {
    let db_canonical_path = dunce::canonicalize(
        app.path()
            .resolve("data/database.db", BaseDirectory::Resource)?,
    )?
    .into_os_string()
    .into_string();
    if let Ok(x) = db_canonical_path {
        Ok(x)
    } else {
        anyhow::bail!("Could not correctly get canonical path.")
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_db_path(app: &mut App) -> anyhow::Result<String> {
    let db_path = app
        .path()
        .resolve("data/database.db", BaseDirectory::Resource)?
        .into_os_string()
        .into_string();
    if let Ok(x) = db_path {
        Ok(x)
    } else {
        anyhow::bail!("Could not correctly get db path.")
    }
}
