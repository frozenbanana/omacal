mod alarms;
mod caldav;
mod commands;
mod config;
mod db;
mod ics;
mod secrets;
mod sync;
mod theme;

use commands::AppState;
use db::Db;
use std::path::Path;
use std::sync::{Arc, Mutex};
use sync::SyncEngine;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

fn ics_arg_from(args: &[String]) -> Option<String> {
    args.iter()
        .find(|a| {
            let lower = a.to_lowercase();
            (lower.ends_with(".ics") || lower.ends_with(".ical"))
                && Path::new(a).is_file()
        })
        .cloned()
}

fn queue_import(app: &AppHandle, path: &str) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("import-ics", path);
}

fn queue_import_cold(app: &AppHandle, path: &str) {
    queue_import(app, path);
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut q) = state.pending_imports.lock() {
            q.push(path.to_string());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::try_init();
    let _ = config::ensure_dirs();

    let db = Arc::new(Db::open(&config::db_path()).expect("open db"));
    // Repair mis-parsed VTIMEZONE RRULEs / PARTSTAT before first UI snapshot
    if db
        .get_meta("ics_vevent_parse_v1")
        .ok()
        .flatten()
        .as_deref()
        != Some("1")
    {
        if let Ok(cfg) = config::load_config() {
            let mut addrs = std::collections::HashMap::new();
            for a in &cfg.accounts {
                addrs.insert(a.id.clone(), a.addresses.clone());
            }
            match db.repair_derived_ics_fields(&addrs) {
                Ok(n) => {
                    log::info!("repaired derived ICS fields on {n} events");
                    let _ = db.set_meta("ics_vevent_parse_v1", "1");
                }
                Err(e) => log::warn!("ICS field repair failed: {e}"),
            }
        }
    }
    let sync = SyncEngine::new(db.clone());
    let state = AppState {
        db: db.clone(),
        sync,
        pending_imports: Arc::new(Mutex::new(Vec::new())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_single_instance::Builder::default()
                .callback(|app, args, _cwd| {
                    if let Some(path) = ics_arg_from(&args) {
                        queue_import(&app, &path);
                    }
                })
                .build(),
        )
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_theme,
            commands::get_config,
            commands::save_config,
            commands::get_snapshot,
            commands::add_account,
            commands::remove_account,
            commands::test_account,
            commands::sync_now,
            commands::list_calendars,
            commands::set_calendar_visible,
            commands::set_calendar_color,
            commands::set_calendar_subscribed,
            commands::set_default_calendar,
            commands::reorder_calendars,
            commands::list_events,
            commands::search_events,
            commands::pending_invites,
            commands::save_event,
            commands::delete_event,
            commands::respond_invite,
            commands::respond_invites_bulk,
            commands::next_event,
            commands::freebusy,
            commands::preview_ics,
            commands::take_pending_imports,
        ])
        .setup(move |app| {
            theme::start_theme_watcher(app.handle().clone());
            alarms::start_alarm_loop(app.handle().clone(), db.clone());

            // Cold-start launch from a file association (e.g. double-clicked .ics)
            let args: Vec<String> = std::env::args().collect();
            if let Some(path) = ics_arg_from(&args) {
                queue_import_cold(app.handle(), &path);
            }
            // background sync loop
            let handle = app.handle().clone();
            let db_sync = db.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                loop {
                    let interval = config::load_config()
                        .map(|c| c.sync_interval_secs)
                        .unwrap_or(300)
                        .max(60);
                    let cfg = config::load_config().ok();
                    if let Some(cfg) = cfg {
                        if !cfg.accounts.is_empty() {
                            let engine = SyncEngine::new(db_sync.clone());
                            let _ = rt.block_on(engine.sync_all(&cfg));
                            let _ = handle.emit("sync-finished", ());
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_secs(interval));
                }
            });

            // tray menu
            if std::env::var("OMARCAL_START_HIDDEN").ok().as_deref() == Some("1") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            let show_i = MenuItem::with_id(app, "show", "Show Omarcal", true, None::<&str>)?;
            let sync_i = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &sync_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Omarcal")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "sync" => {
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app2.state::<AppState>();
                            let cfg = config::load_config().ok();
                            if let Some(cfg) = cfg {
                                let _ = state.sync.sync_all(&cfg).await;
                                let _ = app2.emit("sync-finished", ());
                            }
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Omarcal");
}
