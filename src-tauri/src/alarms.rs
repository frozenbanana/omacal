use crate::db::Db;
use crate::ics;
use chrono::{Duration, Utc};
use notify_rust::Notification;
use std::sync::Arc;
use std::thread;
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Emitter, Manager};

pub fn start_alarm_loop(app: AppHandle, db: Arc<Db>) {
    thread::spawn(move || {
        loop {
            if let Err(e) = tick(&app, &db) {
                log::warn!("alarm tick error: {e}");
            }
            thread::sleep(StdDuration::from_secs(30));
        }
    });
}

fn tick(app: &AppHandle, db: &Db) -> anyhow::Result<()> {
    let now = Utc::now();
    let horizon = now + Duration::hours(24);
    let events = db.list_events(true)?;

    for ev in events {
        let alarms: Vec<crate::db::AlarmInfo> =
            serde_json::from_str(&ev.alarms_json).unwrap_or_default();
        if alarms.is_empty() {
            continue;
        }
        let Some(start_s) = &ev.dtstart else {
            continue;
        };
        // Expand simple non-recurring or use dtstart
        let starts = if let Some(rrule) = &ev.rrule {
            ics::expand_rrule_occurrences(start_s, rrule, now - Duration::hours(1), horizon)
        } else if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(start_s) {
            vec![dt.with_timezone(&Utc)]
        } else {
            continue;
        };

        for start in starts {
            for alarm in &alarms {
                if let Some(trigger_at) = ics::alarm_trigger_at(start, &alarm.trigger) {
                    // fire if within the last 2 minutes window or overdue by < 5 min
                    let delta = now.signed_duration_since(trigger_at);
                    if delta >= Duration::zero() && delta < Duration::minutes(5) {
                        let key = trigger_at.to_rfc3339();
                        if db.mark_alarm_fired(&ev.uid, &key)? {
                            let body = if ev.location.is_empty() {
                                format!("Starts {}", start.format("%H:%M"))
                            } else {
                                format!("{} · {}", start.format("%H:%M"), ev.location)
                            };
                            let _ = Notification::new()
                                .summary(&ev.summary)
                                .body(&body)
                                .appname("Omarcal")
                                .timeout(notify_rust::Timeout::Milliseconds(10000))
                                .show();
                            let _ = app.emit(
                                "alarm-fired",
                                serde_json::json!({
                                    "uid": ev.uid,
                                    "summary": ev.summary,
                                    "start": start.to_rfc3339(),
                                }),
                            );
                            // focus main window if possible
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
