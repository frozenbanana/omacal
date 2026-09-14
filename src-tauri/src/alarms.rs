use crate::db::Db;
use crate::ics;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use notify_rust::Notification;
use std::sync::Arc;
use std::thread;
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Emitter, Manager};

pub fn start_alarm_loop(app: AppHandle, db: Arc<Db>) {
    thread::spawn(move || loop {
        if let Err(e) = tick(&app, &db) {
            log::warn!("alarm tick error: {e}");
        }
        thread::sleep(StdDuration::from_secs(30));
    });
}

fn tick(app: &AppHandle, db: &Db) -> anyhow::Result<()> {
    let now = Utc::now();
    let horizon = now + Duration::hours(24);
    let events = db.list_events(true)?;
    let cfg = crate::config::load_config().ok();
    let default_tz = cfg
        .as_ref()
        .and_then(|config| config.locale.timezone.parse::<chrono_tz::Tz>().ok())
        .unwrap_or_else(ics::default_tz);

    for ev in events {
        let master_alarms: Vec<crate::db::AlarmInfo> =
            serde_json::from_str(&ev.alarms_json).unwrap_or_default();
        let Some(start_s) = &ev.dtstart else {
            continue;
        };
        let overrides: std::collections::HashMap<String, ics::ParsedEvent> =
            ics::parse_recurrence_overrides_with_tz(&ev.raw_ics, &[], Some(default_tz))
                .into_iter()
                .map(|item| (item.recurrence_id, item.event))
                .collect();
        let mut starts: Vec<(String, chrono::DateTime<Utc>)> = if let Some(rrule) = &ev.rrule {
            if ev.all_day {
                ics::expand_all_day_from_raw(
                    &ev.raw_ics,
                    start_s,
                    rrule,
                    (now - Duration::hours(1))
                        .with_timezone(&default_tz)
                        .date_naive(),
                    horizon.with_timezone(&default_tz).date_naive(),
                )
                .into_iter()
                .filter_map(|date| {
                    date.and_hms_opt(0, 0, 0).and_then(|wall| {
                        default_tz
                            .from_local_datetime(&wall)
                            .single()
                            .or_else(|| default_tz.from_local_datetime(&wall).earliest())
                            .map(|dt| (date.format("%Y-%m-%d").to_string(), dt.with_timezone(&Utc)))
                    })
                })
                .collect()
            } else {
                ics::expand_rrule_from_raw(
                    &ev.raw_ics,
                    start_s,
                    rrule,
                    now - Duration::hours(1),
                    horizon,
                )
                .into_iter()
                .map(|start| (start.to_rfc3339(), start))
                .collect()
            }
        } else if let Some(start) = parse_event_start(start_s, default_tz) {
            vec![(start_s.clone(), start)]
        } else {
            continue;
        };
        let seen: std::collections::HashSet<String> = starts
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect();
        for (identity, occurrence) in &overrides {
            if seen.contains(identity) || occurrence.status.as_deref() == Some("CANCELLED") {
                continue;
            }
            let Some(start) = occurrence
                .dtstart
                .as_deref()
                .and_then(|value| parse_event_start(value, default_tz))
            else {
                continue;
            };
            if start >= now - Duration::hours(1) && start <= horizon {
                starts.push((identity.clone(), start));
            }
        }

        for (recurrence_id, master_start) in starts {
            let occurrence = overrides.get(&recurrence_id);
            if occurrence.and_then(|event| event.status.as_deref()) == Some("CANCELLED") {
                continue;
            }
            let start = occurrence
                .and_then(|event| event.dtstart.as_deref())
                .and_then(|value| parse_event_start(value, default_tz))
                .unwrap_or(master_start);
            let alarms = occurrence
                .map(|event| event.alarms.as_slice())
                .unwrap_or(master_alarms.as_slice());
            let summary = occurrence
                .map(|event| event.summary.as_str())
                .unwrap_or(ev.summary.as_str());
            let location = occurrence
                .map(|event| event.location.as_str())
                .unwrap_or(ev.location.as_str());
            for alarm in alarms {
                if let Some(trigger_at) = ics::alarm_trigger_at(start, &alarm.trigger) {
                    // fire if within the last 2 minutes window or overdue by < 5 min
                    let delta = now.signed_duration_since(trigger_at);
                    if delta >= Duration::zero() && delta < Duration::minutes(5) {
                        let key = trigger_at.to_rfc3339();
                        if db.mark_alarm_fired(&ev.uid, &key)? {
                            let body = if location.is_empty() {
                                format!("Starts {}", start.format("%H:%M"))
                            } else {
                                format!("{} · {}", start.format("%H:%M"), location)
                            };
                            let _ = Notification::new()
                                .summary(summary)
                                .body(&body)
                                .appname("Omacal")
                                .timeout(notify_rust::Timeout::Milliseconds(10000))
                                .show();
                            let _ = app.emit(
                                "alarm-fired",
                                serde_json::json!({
                                    "uid": ev.uid,
                                    "summary": summary,
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

fn parse_event_start(value: &str, timezone: chrono_tz::Tz) -> Option<chrono::DateTime<Utc>> {
    if let Ok(value) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(value.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(&value[..10.min(value.len())], "%Y-%m-%d").ok()?;
    let wall = date.and_hms_opt(0, 0, 0)?;
    timezone
        .from_local_datetime(&wall)
        .single()
        .or_else(|| timezone.from_local_datetime(&wall).earliest())
        .map(|value| value.with_timezone(&Utc))
}
