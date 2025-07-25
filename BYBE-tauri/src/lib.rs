use bybe::InitializeLogResponsibility;
use std::thread;
use log::{info, warn};
use tauri::path::BaseDirectory;
use tauri::{App, Manager};
use tauri_plugin_updater::{UpdaterExt};


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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                update(handle).await.unwrap();
            });

            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();
            // Get Environmental Variables
            let env_path = app
                .path()
                .resolve(".env", BaseDirectory::Resource)
                .expect("Should find BYBE env variables inside resources")
                .into_os_string()
                .into_string()
                .ok();
            // get DB
            let db_path = get_db_path(app).ok();
            let jsons_path = get_jsons_path(app).ok();
            thread::spawn(move || {
                bybe::start(env_path, db_path, jsons_path, InitializeLogResponsibility::Delegated)
                    .expect("Backend should be able to startup, port or ip busy?");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "windows")]
pub fn get_db_path(app: &mut App) -> anyhow::Result<String> {
    let db_canonical_path =
        dunce::canonicalize(app.path().resolve("data/database.db", BaseDirectory::Resource)?)?
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

#[cfg(target_os = "windows")]
pub fn get_jsons_path(app: &mut App) -> anyhow::Result<(String, String)> {
    let name_path =
        dunce::canonicalize(app.path().resolve("data/names.db", BaseDirectory::Resource)?)?
            .into_os_string()
            .into_string();
    let nickname_path =
        dunce::canonicalize(app.path().resolve("data/nicknames.db", BaseDirectory::Resource)?)?
            .into_os_string()
            .into_string();
    if let Ok(names) = name_path && let Ok(nicknames) = nickname_path {
        Ok((names, nicknames))
    } else {
        anyhow::bail!("Could not correctly get name or nicknames path.")
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_jsons_path(app: &mut App) -> anyhow::Result<(String, String)> {
    let name_path = app
        .path()
        .resolve("data/names.json", BaseDirectory::Resource)?
        .into_os_string()
        .into_string();
    let nickname_path = app
        .path()
        .resolve("data/nickname.json", BaseDirectory::Resource)?
        .into_os_string()
        .into_string();
    if let Ok(names) = name_path && let Ok(nicknames) = nickname_path {
        Ok((names, nicknames))
    } else {
        anyhow::bail!("Could not correctly get name or nicknames path.")
    }
}


async fn update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    match app.updater()?.check().await? {
        None => {
            info!("No update found");
        }
        Some(update) => {
            let mut downloaded = 0;
            // alternatively we could also call update.download() and update.install() separately
            update
                .download_and_install(
                    |chunk_length, content_length| {
                        downloaded += chunk_length;
                        info!("Downloaded {downloaded} from {content_length:?}");
                    },
                    || {
                        info!("Download finished");
                    },
                )
                .await?;

            info!("Update installed");
            warn!("Restarting app...");
            app.restart();
        }
    }
    Ok(())
}
