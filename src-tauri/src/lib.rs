//! Topic Desk Studio native application composition and lifecycle setup.

mod browser_control;
mod browser_profile;
mod catalog;
mod collector;
mod commands;
mod credential_cipher;
mod database;
mod desktop;
mod error;
mod identity;
mod models;
mod repository;
mod storage;
mod translator;

use commands::{
    backup_storage, browser_request, collect_xiaohongshu_session, get_model_settings,
    get_network_settings, get_storage_status, get_ui_preferences, list_topics, open_data_directory,
    optimize_storage, refresh_topics, restore_latest_backup, save_model_settings,
    save_network_settings, save_ui_preferences, set_platform_enabled, set_topic_queued,
    translate_topic, AppState,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;

/// Build and run the Tauri application on the current desktop platform.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("无法解析应用数据目录：{error}"))?;
            std::fs::create_dir_all(&data_dir)
                .map_err(|error| format!("无法创建应用数据目录：{error}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))
                    .map_err(|error| format!("无法限制应用数据目录权限：{error}"))?;
            }
            app.manage(browser_profile::open(data_dir.join("browser.sqlite"))?);
            let database_path = data_dir.join("topic-desk.sqlite");
            let database =
                database::open_database(&database_path).map_err(|error| error.to_string())?;
            let refreshing = Arc::new(AtomicBool::new(false));
            app.manage(AppState {
                database: std::sync::Mutex::new(database),
                database_path: database_path.clone(),
                refreshing: Arc::clone(&refreshing),
            });

            // A detached scheduler keeps desktop refreshes independent from WebView visibility.
            let event_app = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5));
                let mut trigger = "startup";
                loop {
                    if refreshing
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        commands::emit_collection_status(
                            &event_app,
                            "started",
                            trigger,
                            "正在自动采集数据…".into(),
                            0,
                            0,
                        );
                        let result = collector::collect_all(&database_path, trigger);
                        refreshing.store(false, Ordering::Release);
                        match result {
                            Ok(stats) => commands::emit_collection_status(
                                &event_app,
                                "finished",
                                trigger,
                                format!(
                                    "自动采集完成：新增 {}，更新 {}",
                                    stats.inserted, stats.updated
                                ),
                                stats.inserted,
                                stats.updated,
                            ),
                            Err(error) => commands::emit_collection_status(
                                &event_app,
                                "failed",
                                trigger,
                                error.to_string(),
                                0,
                                0,
                            ),
                        }
                    }
                    trigger = "schedule";
                    std::thread::sleep(Duration::from_secs(10 * 60));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_topics,
            refresh_topics,
            collect_xiaohongshu_session,
            get_model_settings,
            save_model_settings,
            get_ui_preferences,
            save_ui_preferences,
            get_network_settings,
            save_network_settings,
            get_storage_status,
            backup_storage,
            restore_latest_backup,
            optimize_storage,
            open_data_directory,
            set_platform_enabled,
            set_topic_queued,
            translate_topic,
            browser_request,
            browser_control::browser_control
        ])
        .run(tauri::generate_context!())
        .expect("Topic Desk Studio failed to start");
}
