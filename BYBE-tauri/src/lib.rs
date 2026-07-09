use bybe_backend::{InitializeLogResponsibility, StartOptions, StartupState};
use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::path::BaseDirectory;
use tauri::webview::WebviewWindowBuilder;
use tauri::{App, Manager, RunEvent, Url, WebviewUrl, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::oneshot;

fn db_version_marker_path(db_data_dir: &str) -> String {
    format!("{db_data_dir}.version")
}

fn db_setup_matches_version(db_data_dir: &str, current_version: &str) -> bool {
    std::fs::read_to_string(db_version_marker_path(db_data_dir))
        .is_ok_and(|marker| marker.trim() == current_version)
}

fn show_setup_failed_dialog_and_quit(app_handle: &tauri::AppHandle) {
    app_handle
        .dialog()
        .message(
            "BYBE could not finish setting up its database. \
             Please restart the app; if the problem persists, check the logs.",
        )
        .title("Setup failed")
        .kind(MessageDialogKind::Error)
        .blocking_show();
    app_handle.exit(1);
}

const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

struct BackendHandle {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
    db_data_dir: String,
}

impl BackendHandle {
    fn shutdown(&self) {
        if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        let Some(handle) = self.join_handle.lock().unwrap().take() else {
            return;
        };

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });

        if done_rx.recv_timeout(SHUTDOWN_JOIN_TIMEOUT).is_err() {
            warn!(
                "Backend did not shut down within {SHUTDOWN_JOIN_TIMEOUT:?}; force-killing Postgres directly"
            );
            if let Err(e) =
                bybe_backend::force_kill_postgres(std::path::Path::new(&self.db_data_dir))
            {
                warn!("Could not force-kill stray Postgres process: {e}");
            }
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
                // sqlx logs the full SQL text for any "slow" statement at Warn,
                // which would otherwise dump the entire multi-thousand-line Clean
                // setup script into the log in one message.
                .level_for("sqlx::query", log::LevelFilter::Error)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let main_window = app
                .get_webview_window("main")
                .expect("Should be able to open main_window");

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                update(handle)
                    .await
                    .expect("Should be able to handle update");
            });

            // Get Environmental Variables
            let env_path = app
                .path()
                .resolve(".env", BaseDirectory::Resource)
                .expect("Should find BYBE env variables inside resources")
                .into_os_string()
                .into_string()
                .ok();
            let jsons_path = get_jsons_path(app).ok();
            let db_data_dir = get_db_data_dir_path(app)
                .expect("Should be able to resolve database data directory");
            let (shutdown_tx, shutdown_rx) = oneshot::channel();

            let current_version = app.package_info().version.to_string();
            let first_startup = !bybe_backend::db_initialized(&db_data_dir)
                || !db_setup_matches_version(&db_data_dir, &current_version);

            let sql_path = first_startup.then(|| get_sql_dump_path(app).ok()).flatten();

            let startup_state = if first_startup {
                StartupState::Clean
            } else {
                StartupState::Persistent
            };
            // Always wait for the backend's readiness signal, not just on first
            // launch: existing db data can still fail to start (e.g. left in
            // a dirty state by an unclean shutdown), and without this the normal
            // boot path used to show the main window unconditionally, with no
            // feedback at all if the backend then silently died in its thread.
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();

            if first_startup {
                let splash_html_path = get_splash_html_path(app)
                    .expect("Should be able to resolve splash screen resource");
                let splash_url = Url::from_file_path(&splash_html_path)
                    .expect("splash html path should be a valid file url");
                let mut splash_builder =
                    WebviewWindowBuilder::new(app, "splash", WebviewUrl::External(splash_url))
                        .title("BYBE - Setting up")
                        .inner_size(1280.0, 720.0)
                        .resizable(false)
                        .decorations(false)
                        .center()
                        .always_on_top(false)
                        .devtools(false);
                if let Some(icon) = app.default_window_icon().cloned() {
                    splash_builder = splash_builder
                        .icon(icon)
                        .expect("Should be able to set splash window icon");
                }
                let splash = splash_builder
                    .build()
                    .expect("Should be able to create splash window");

                // Undecorated windows have no close button, but the OS-level
                // shortcuts (Alt+F4, Cmd+Q, etc) still send a close request
                // `splash.close()` (called below once setup succeeds/fails) fires
                // this exact same `CloseRequested` event, so we set a flag.
                // the flag is only "true" while a close is expected
                // to mean "the user wants to quit".
                let user_can_quit = Arc::new(AtomicBool::new(true));
                let user_can_quit_for_handler = user_can_quit.clone();
                let app_handle_for_splash = app.handle().clone();
                splash.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { .. } = event
                        && user_can_quit_for_handler.load(Ordering::SeqCst)
                    {
                        app_handle_for_splash.exit(0);
                    }
                });

                let marker_path = db_version_marker_path(&db_data_dir);
                let app_handle_for_failure = app.handle().clone();
                thread::spawn(move || {
                    let setup_succeeded = ready_rx.recv().is_ok();
                    user_can_quit.store(false, Ordering::SeqCst);
                    if !setup_succeeded {
                        show_setup_failed_dialog_and_quit(&app_handle_for_failure);
                        return;
                    }
                    let _ = std::fs::write(marker_path, &current_version);
                    // Window operations must happen on the main thread (some
                    // platforms, e.g. WebKitGTK on Linux, will crash otherwise)
                    let _ = app_handle_for_failure.run_on_main_thread(move || {
                        let _ = main_window.show();
                        let _ = splash.hide();
                    });
                });
            } else {
                let app_handle_for_failure = app.handle().clone();
                thread::spawn(move || {
                    if ready_rx.recv().is_err() {
                        show_setup_failed_dialog_and_quit(&app_handle_for_failure);
                        return;
                    }
                    let _ = app_handle_for_failure.run_on_main_thread(move || {
                        let _ = main_window.show();
                        #[cfg(debug_assertions)]
                        main_window.open_devtools();
                    });
                });
            }

            let db_data_dir_for_backend_handle = db_data_dir.clone();
            let join_handle = thread::spawn(move || {
                bybe_backend::start(StartOptions {
                    env_location: env_path,
                    sql_location: sql_path,
                    jsons_location: jsons_path,
                    db_data_dir: Some(db_data_dir),
                    shutdown_signal: Some(shutdown_rx),
                    init_log_resp: InitializeLogResponsibility::Delegated,
                    startup_state_override: Some(startup_state),
                    ready_signal: Some(ready_tx),
                })
                .expect("Backend should be able to startup, port or ip busy?");
            });

            let backend_handle = Arc::new(BackendHandle {
                shutdown_tx: Mutex::new(Some(shutdown_tx)),
                join_handle: Mutex::new(Some(join_handle)),
                db_data_dir: db_data_dir_for_backend_handle,
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
    dunce::canonicalize(
        app.path()
            .resolve("data/bybe_pglite.sql", BaseDirectory::Resource)?,
    )?
    .into_os_string()
    .into_string()
    .map_err(|x| anyhow::anyhow!("Error fetching sql dump: {:?}", x))
}

#[cfg(not(target_os = "windows"))]
pub fn get_sql_dump_path(app: &mut App) -> anyhow::Result<String> {
    app.path()
        .resolve("data/bybe_pglite.sql", BaseDirectory::Resource)?
        .into_os_string()
        .into_string()
        .map_err(|x| anyhow::anyhow!("Error fetching sql dump: {:?}", x))
}

#[cfg(target_os = "windows")]
pub fn get_jsons_path(app: &mut App) -> anyhow::Result<(String, String)> {
    let name_path = dunce::canonicalize(
        app.path()
            .resolve("data/names.json", BaseDirectory::Resource)?,
    )?
    .into_os_string()
    .into_string();
    let nickname_path = dunce::canonicalize(
        app.path()
            .resolve("data/nicknames.json", BaseDirectory::Resource)?,
    )?
    .into_os_string()
    .into_string();
    if let Ok(names) = name_path
        && let Ok(nicknames) = nickname_path
    {
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
    if let Ok(names) = name_path
        && let Ok(nicknames) = nickname_path
    {
        Ok((names, nicknames))
    } else {
        anyhow::bail!("Could not correctly get name or nicknames path.")
    }
}

#[cfg(target_os = "windows")]
pub fn get_db_data_dir_path(app: &mut App) -> anyhow::Result<String> {
    let app_data_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&app_data_dir)?;
    dunce::canonicalize(app_data_dir)?
        .join(".postgres-data")
        .into_os_string()
        .into_string()
        .map_err(|x| anyhow::anyhow!("Error fetching db data folder path: {:?}", x))
}

#[cfg(not(target_os = "windows"))]
pub fn get_db_data_dir_path(app: &mut App) -> anyhow::Result<String> {
    app.path()
        .app_local_data_dir()?
        .join(".postgres-data")
        .into_os_string()
        .into_string()
        .map_err(|x| anyhow::anyhow!("Error fetching db data folder: {:?}", x))
}

#[cfg(target_os = "windows")]
pub fn get_splash_html_path(app: &mut App) -> anyhow::Result<String> {
    Ok(dunce::canonicalize(
        app.path()
            .resolve("assets/splash.html", BaseDirectory::Resource)?,
    )?
    .into_os_string()
    .into_string()
    .map_err(|x| anyhow::anyhow!("Error loading the splash html file from disk: {:?}", x))?)
}

#[cfg(not(target_os = "windows"))]
pub fn get_splash_html_path(app: &mut App) -> anyhow::Result<String> {
    app.path()
        .resolve("assets/splash.html", BaseDirectory::Resource)?
        .into_os_string()
        .into_string()
        .map_err(|x| anyhow::anyhow!("Error loading the splash html file from disk: {:?}", x))
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
