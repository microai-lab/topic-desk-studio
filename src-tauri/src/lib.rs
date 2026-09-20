//! Topic Desk Studio native application composition and lifecycle setup.

mod browser_control;
mod browser_profile;
mod catalog;
mod collector;
mod commands;
mod database;
mod desktop;
mod error;
mod identity;
mod models;
mod repository;
mod translator;

use commands::{
    browser_request, get_model_settings, list_topics, refresh_topics, save_model_settings,
    set_platform_enabled, set_topic_queued, translate_topic, AppState,
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
            app.manage(browser_profile::open(data_dir.join("browser.sqlite"))?);
            let database_path = data_dir.join("topic-desk.sqlite");
            let database =
                database::open_database(&database_path).map_err(|error| error.to_string())?;
            let refreshing = Arc::new(AtomicBool::new(false));
            app.manage(AppState {
                database: std::sync::Mutex::new(database),
                database_path: database_path.clone(),
                refreshing: Arc::clone(&refreshing),
                api_key_cache: Arc::new(std::sync::Mutex::new(
                    translator::CredentialCache::default(),
                )),
            });

            // A detached scheduler keeps desktop refreshes independent from WebView visibility.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5));
                let mut trigger = "startup";
                loop {
                    if refreshing
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        let _ = collector::collect_all(&database_path, trigger);
                        refreshing.store(false, Ordering::Release);
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
            get_model_settings,
            save_model_settings,
            set_platform_enabled,
            set_topic_queued,
            translate_topic,
            browser_request,
            browser_control::browser_control
        ])
        .run(tauri::generate_context!())
        .expect("Topic Desk Studio failed to start");
}
