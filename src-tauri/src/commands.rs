// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

use serde::Serialize;
use specta::Type;
use std::{collections::HashMap, env, path::PathBuf, sync::Arc};
use tauri::async_runtime;
use tokio::fs;

// Re-export for lib.rs to register commands
pub use crate::{
    errors::{TauriError, TauriResult},
    services::{
        self, aria2c, ffmpeg,
        login::{
            self, exit, pwd_login, refresh_cookie, scan_login, sms_login, stop_login, switch_cookie,
        },
        queue::{
            self,
            atomics::QueueType,
            ctrl_event,
            open_folder,
            plan_scheduler,
            process_scheduler,
            scheduler::SchedulerView,
            // update_max_conc,
            submit_task,
            task::TaskView,
        },
    },
    shared::{self, get_app_handle, set_window, HEADERS, READY},
    storage::{
        self,
        config::{self, CacheKey},
        cookies, db, queue as queues, schedulers, tasks,
    },
};

#[derive(Serialize, Type)]
pub struct InitData {
    version: String,
    hash: String,
    config: Arc<config::Settings>,
    tasks: HashMap<String, TaskView>,
    schedulers: HashMap<String, SchedulerView>,
    queue: HashMap<QueueType, Vec<String>>,
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_size(key: CacheKey, event: tauri::ipc::Channel<u64>) -> TauriResult<()> {
    let path = config::read().get_cache(&key)?;
    if !path.exists() {
        log::warn!("Cache {key:?} path does not exist: {}", path.display());
        event.send(0)?;
        return Ok(());
    }
    let mut bytes = 0u64;
    let mut count = 0;
    for entry in walkdir::WalkDir::new(&path)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() {
            match fs::metadata(path).await {
                Ok(meta) => {
                    bytes += meta.len();
                    count += 1;
                    if count > 200 {
                        event.send(bytes)?;
                        count = 0;
                    }
                }
                Err(_) => continue,
            }
        }
    }
    event.send(bytes)?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn clean_cache(key: CacheKey) -> TauriResult<()> {
    let path = config::read().get_cache(&key)?;
    if !path.exists() {
        log::warn!("Cache {key:?} path does not exist: {}", path.display());
        return Ok(());
    }
    if key == CacheKey::Database {
        if let Err(e) = db::close_db().await {
            log::warn!("Failed to close database before cleaning: {e}");
            return Ok(());
        }
        if let Err(e) = fs::remove_file(&path).await {
            log::warn!("Failed to remove database cache: {e}");
            return Ok(());
        }
        let app = get_app_handle();
        app.restart();
    }
    let mut entries = match fs::read_dir(&path).await {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Failed to clean cache {key:?}: {e}");
            return Ok(());
        }
    };
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                let path = entry.path();
                async_runtime::spawn(async move {
                    if let Err(e) = if path.is_dir() {
                        fs::remove_dir_all(&path).await
                    } else {
                        fs::remove_file(&path).await
                    } {
                        log::warn!("Failed to clean cache entry {}: {e}", path.display());
                    }
                });
            }
            Ok(None) => break,
            Err(e) => {
                log::warn!("Failed to read cache directory: {e}");
                break;
            }
        }
    }
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn open_cache(key: CacheKey) -> TauriResult<()> {
    let path = config::read().get_cache(&key)?;
    if !path.exists() {
        fs::create_dir_all(&path).await?;
    }
    tauri_plugin_opener::open_path(path, None::<&str>)?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn config_write(settings: serde_json::Map<String, serde_json::Value>) -> TauriResult<()> {
    config::write(settings).await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn db_export(output: PathBuf) -> TauriResult<()> {
    db::export(output).await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn db_import(app: tauri::AppHandle, input: PathBuf) -> TauriResult<()> {
    db::import(input).await?;
    app.restart();
}

#[tauri::command(async)]
#[specta::specta]
pub async fn export_data(output: PathBuf, data: serde_json::Value) -> TauriResult<()> {
    let json = serde_json::to_string_pretty(&data)?;
    fs::write(output, json.as_bytes()).await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn meta(app: tauri::AppHandle) -> TauriResult<InitData> {
    let version = app.package_info().version.to_string();
    let hash = env!("GIT_HASH").to_string();
    let config = config::read();

    let tasks = tasks::load().await?;
    let schedulers = schedulers::load().await?;
    let queue = queues::load().await?;

    Ok(InitData {
        version,
        hash,
        config,
        tasks,
        schedulers,
        queue,
    })
}

#[tauri::command(async)]
#[specta::specta]
pub async fn init() -> TauriResult<()> {
    if READY.set(()).is_err() {
        #[cfg(not(debug_assertions))]
        return Err(anyhow::anyhow!("403 Forbidden").into());
    }
    login::stop_login();
    login::get_buvid().await?;
    login::get_bili_ticket().await?;
    login::get_uuid().await?;
    HEADERS.refresh().await?;
    Ok(())
}
