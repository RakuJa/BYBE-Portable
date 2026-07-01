use bybe_backend::InitializeLogResponsibility;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use log::{info, warn};
use tauri::path::BaseDirectory;
use tauri::{App, Manager, RunEvent};
use tauri_plugin_updater::{UpdaterExt};
use tokio::sync::oneshot;

/// Lets us ask the backend thread (actix + pglite) to shut down gracefully
/// instead of leaving an orphaned postgres process behind when the app exits,
/// whether via the window close button or Ctrl+C in a dev terminal.
struct BackendHandle {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

impl BackendHandle {
    fn shutdown(&self) {
        if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.join_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    let app = builder
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
            let sql_path = get_sql_dump_path(app).ok();
            let jsons_path = get_jsons_path(app).ok();
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let join_handle = thread::spawn(move || {
                bybe_backend::start(
                    env_path,
                    sql_path,
                    jsons_path,
                    Some(shutdown_rx),
                    InitializeLogResponsibility::Delegated,
                )
                .expect("Backend should be able to startup, port or ip busy?");
            });

            let backend_handle = Arc::new(BackendHandle {
                shutdown_tx: Mutex::new(Some(shutdown_tx)),
                join_handle: Mutex::new(Some(join_handle)),
            });
            app.manage(backend_handle.clone());

            // Ctrl+C in a dev terminal kills the process directly, bypassing
            // Tauri's own exit event, so it needs its own graceful shutdown hook.
            ctrlc::set_handler(move || {
                backend_handle.shutdown();
                std::process::exit(0);
            })
            .expect("Should be able to register ctrl-c handler");

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event
            && let Some(backend_handle) = app_handle.try_state::<Arc<BackendHandle>>()
        {
            backend_handle.shutdown();
        }
    });
}

#[cfg(target_os = "windows")]
pub fn get_sql_dump_path(app: &mut App) -> anyhow::Result<String> {
    let sql_canonical_path = dunce::canonicalize(
        app.path().resolve("data/bybe_pglite.sql", BaseDirectory::Resource)?,
    )?
    .into_os_string()
    .into_string();
    if let Ok(x) = sql_canonical_path {
        Ok(x)
    } else {
        anyhow::bail!("Could not correctly get canonical path.")
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_sql_dump_path(app: &mut App) -> anyhow::Result<String> {
    let sql_path = app
        .path()
        .resolve("data/bybe_pglite.sql", BaseDirectory::Resource)?
        .into_os_string()
        .into_string();
    if let Ok(x) = sql_path {
        Ok(x)
    } else {
        anyhow::bail!("Could not correctly get sql dump path.")
    }
}

#[cfg(target_os = "windows")]
pub fn get_jsons_path(app: &mut App) -> anyhow::Result<(String, String)> {
    let name_path =
        dunce::canonicalize(app.path().resolve("data/names.json", BaseDirectory::Resource)?)?
            .into_os_string()
            .into_string();
    let nickname_path =
        dunce::canonicalize(app.path().resolve("data/nicknames.json", BaseDirectory::Resource)?)?
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
        .resolve("data/nicknames.json", BaseDirectory::Resource)?
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
