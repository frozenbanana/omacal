use crate::config::{self, AccountConfig, AppConfig};
use crate::db::{AlarmInfo, AttendeeInfo, CalendarRow, Db, EventRow};
use crate::ics::{self, EventInput};
use crate::secrets;
use crate::sync::{SyncEngine, SyncReport};
use crate::theme::{self, ThemeColors};
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;
use uuid::Uuid;

pub struct AppState {
    pub db: Arc<Db>,
    pub sync: SyncEngine,
    pub pending_imports: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Serialize)]
pub struct AppSnapshot {
    pub config: AppConfig,
    pub calendars: Vec<CalendarRow>,
    pub events: Vec<UiEvent>,
    pub pending_invites: Vec<UiEvent>,
    pub theme: ThemeColors,
    pub last_sync: Option<String>,
    pub last_sync_error: Option<String>,
    pub default_calendar_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiEvent {
    pub id: i64,
    pub calendar_id: i64,
    pub href: String,
    pub etag: Option<String>,
    pub uid: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub all_day: bool,
    pub rrule: Option<String>,
    pub color: String,
    pub calendar_name: String,
    pub status: Option<String>,
    pub organizer: Option<String>,
    pub attendees: Vec<AttendeeInfo>,
    pub alarms: Vec<AlarmInfo>,
    pub my_partstat: Option<String>,
    pub readonly: bool,
    /// For expanded recurring occurrences, the master DTSTART (UTC RFC3339 or date). Used for editing series.
    pub master_start: Option<String>,
    pub master_end: Option<String>,
    /// Original recurrence slot. Unlike `start`, this does not change when an instance is moved.
    pub recurrence_id: Option<String>,
    /// Editable master values used when the user chooses to edit the entire series.
    pub series_master: Option<SeriesMaster>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesMaster {
    pub summary: String,
    pub description: String,
    pub location: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub all_day: bool,
    pub rrule: Option<String>,
    pub attendees: Vec<AttendeeInfo>,
    pub alarms: Vec<AlarmInfo>,
}

fn to_ui(ev: EventRow, cal: Option<&CalendarRow>) -> UiEvent {
    let attendees: Vec<AttendeeInfo> = serde_json::from_str(&ev.attendees_json).unwrap_or_default();
    let alarms: Vec<AlarmInfo> = serde_json::from_str(&ev.alarms_json).unwrap_or_default();
    UiEvent {
        id: ev.id,
        calendar_id: ev.calendar_id,
        href: ev.href,
        etag: ev.etag,
        uid: ev.uid,
        title: ev.summary,
        description: ev.description,
        location: ev.location,
        start: ev.dtstart.clone(),
        end: ev.dtend.clone(),
        all_day: ev.all_day,
        rrule: ev.rrule.clone(),
        color: cal
            .map(|c| c.color.clone())
            .unwrap_or_else(|| "#829dd4".into()),
        calendar_name: cal.map(|c| c.displayname.clone()).unwrap_or_default(),
        status: ev.status,
        organizer: ev.organizer,
        attendees,
        alarms,
        my_partstat: ev.my_partstat,
        readonly: cal.map(|c| c.readonly).unwrap_or(false),
        master_start: ev.dtstart,
        master_end: ev.dtend,
        recurrence_id: None,
        series_master: None,
    }
}

fn series_master(ev: &EventRow) -> SeriesMaster {
    SeriesMaster {
        summary: ev.summary.clone(),
        description: ev.description.clone(),
        location: ev.location.clone(),
        start: ev.dtstart.clone(),
        end: ev.dtend.clone(),
        all_day: ev.all_day,
        rrule: ev.rrule.clone(),
        attendees: serde_json::from_str(&ev.attendees_json).unwrap_or_default(),
        alarms: serde_json::from_str(&ev.alarms_json).unwrap_or_default(),
    }
}

fn row_with_override(master: &EventRow, event: &ics::ParsedEvent) -> EventRow {
    EventRow {
        summary: event.summary.clone(),
        description: event.description.clone(),
        location: event.location.clone(),
        dtstart: event.dtstart.clone(),
        dtend: event.dtend.clone(),
        all_day: event.all_day,
        status: event.status.clone(),
        organizer: event.organizer.clone(),
        attendees_json: serde_json::to_string(&event.attendees).unwrap_or_else(|_| "[]".into()),
        alarms_json: serde_json::to_string(&event.alarms).unwrap_or_else(|_| "[]".into()),
        my_partstat: event.my_partstat.clone(),
        ..master.clone()
    }
}

fn build_events(db: &Db, cfg: &AppConfig) -> Result<Vec<UiEvent>, String> {
    let calendars = db.list_calendars().map_err(|e| e.to_string())?;
    let map: std::collections::HashMap<i64, CalendarRow> =
        calendars.into_iter().map(|c| (c.id, c)).collect();
    let events = db.list_events(true).map_err(|e| e.to_string())?;
    let range_start = chrono::Utc::now() - chrono::Duration::days(60);
    let range_end = chrono::Utc::now() + chrono::Duration::days(400);
    let mut out = Vec::new();
    for e in events {
        let cal = map.get(&e.calendar_id).cloned();
        let addresses = cal
            .as_ref()
            .and_then(|calendar| {
                cfg.accounts
                    .iter()
                    .find(|account| account.id == calendar.account_id)
            })
            .map(|account| account.addresses.as_slice())
            .unwrap_or(&[]);
        let default_tz = cfg.locale.timezone.parse::<chrono_tz::Tz>().ok();
        let overrides = ics::parse_recurrence_overrides_with_tz(&e.raw_ics, addresses, default_tz);
        let override_map: std::collections::HashMap<String, ics::ParsedEvent> = overrides
            .into_iter()
            .map(|item| (item.recurrence_id, item.event))
            .collect();
        let master = series_master(&e);
        if let (Some(rrule), Some(start)) = (e.rrule.clone(), e.dtstart.clone()) {
            if start.contains('T') {
                let duration = match (&e.dtstart, &e.dtend) {
                    (Some(s), Some(en)) => {
                        let a = chrono::DateTime::parse_from_rfc3339(s).ok();
                        let b = chrono::DateTime::parse_from_rfc3339(en).ok();
                        match (a, b) {
                            (Some(a), Some(b)) => b.signed_duration_since(a),
                            _ => chrono::Duration::hours(1),
                        }
                    }
                    _ => chrono::Duration::hours(1),
                };
                // Prefer DST-safe wall+TZ expansion if raw contains TZID
                let occurrences = crate::ics::expand_rrule_from_raw(
                    &e.raw_ics,
                    &start,
                    &rrule,
                    range_start,
                    range_end,
                );
                if !occurrences.is_empty() {
                    let mut seen_overrides = std::collections::HashSet::new();
                    for occ in occurrences {
                        let recurrence_id = occ.to_rfc3339();
                        seen_overrides.insert(recurrence_id.clone());
                        let occurrence_row = override_map
                            .get(&recurrence_id)
                            .map(|event| row_with_override(&e, event))
                            .unwrap_or_else(|| EventRow {
                                dtstart: Some(recurrence_id.clone()),
                                dtend: Some((occ + duration).to_rfc3339()),
                                ..e.clone()
                            });
                        if occurrence_row.status.as_deref() == Some("CANCELLED") {
                            continue;
                        }
                        let mut ui = to_ui(occurrence_row, cal.as_ref());
                        // keep master start/end for editing series
                        ui.master_start = e.dtstart.clone();
                        ui.master_end = e.dtend.clone();
                        ui.recurrence_id = Some(recurrence_id);
                        ui.series_master = Some(master.clone());
                        // unique id for FC: keep base id, encode occurrence in uid display
                        ui.uid = format!("{}::{}", e.uid, occ.timestamp());
                        out.push(ui);
                    }
                    for (recurrence_id, event) in &override_map {
                        if seen_overrides.contains(recurrence_id)
                            || event.status.as_deref() == Some("CANCELLED")
                        {
                            continue;
                        }
                        let Some(display_start) = event
                            .dtstart
                            .as_deref()
                            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                            .map(|value| value.with_timezone(&chrono::Utc))
                        else {
                            continue;
                        };
                        if display_start < range_start || display_start > range_end {
                            continue;
                        }
                        let mut ui = to_ui(row_with_override(&e, event), cal.as_ref());
                        ui.master_start = e.dtstart.clone();
                        ui.master_end = e.dtend.clone();
                        ui.recurrence_id = Some(recurrence_id.clone());
                        ui.series_master = Some(master.clone());
                        ui.uid = format!("{}::{}", e.uid, recurrence_id);
                        out.push(ui);
                    }
                    continue;
                }
            } else if let Ok(start_date) = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d") {
                let duration_days = match (&e.dtstart, &e.dtend) {
                    (Some(_), Some(end)) => chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d")
                        .ok()
                        .map(|end| (end - start_date).num_days())
                        .filter(|days| *days > 0)
                        .unwrap_or(1),
                    _ => 1,
                };
                let occurrences = ics::expand_all_day_from_raw(
                    &e.raw_ics,
                    &start,
                    &rrule,
                    range_start.date_naive(),
                    range_end.date_naive(),
                );
                if !occurrences.is_empty() {
                    let mut seen_overrides = std::collections::HashSet::new();
                    for date in occurrences {
                        let recurrence_id = date.format("%Y-%m-%d").to_string();
                        seen_overrides.insert(recurrence_id.clone());
                        let occurrence_row = override_map
                            .get(&recurrence_id)
                            .map(|event| row_with_override(&e, event))
                            .unwrap_or_else(|| EventRow {
                                dtstart: Some(recurrence_id.clone()),
                                dtend: Some(
                                    (date + chrono::Duration::days(duration_days))
                                        .format("%Y-%m-%d")
                                        .to_string(),
                                ),
                                ..e.clone()
                            });
                        if occurrence_row.status.as_deref() == Some("CANCELLED") {
                            continue;
                        }
                        let mut ui = to_ui(occurrence_row, cal.as_ref());
                        ui.master_start = e.dtstart.clone();
                        ui.master_end = e.dtend.clone();
                        ui.recurrence_id = Some(recurrence_id.clone());
                        ui.series_master = Some(master.clone());
                        ui.uid = format!("{}::{}", e.uid, recurrence_id);
                        out.push(ui);
                    }
                    for (recurrence_id, event) in &override_map {
                        if seen_overrides.contains(recurrence_id)
                            || event.status.as_deref() == Some("CANCELLED")
                        {
                            continue;
                        }
                        let Some(display_date) = event.dtstart.as_deref().and_then(|value| {
                            chrono::NaiveDate::parse_from_str(
                                &value[..10.min(value.len())],
                                "%Y-%m-%d",
                            )
                            .ok()
                        }) else {
                            continue;
                        };
                        if display_date < range_start.date_naive()
                            || display_date > range_end.date_naive()
                        {
                            continue;
                        }
                        let mut ui = to_ui(row_with_override(&e, event), cal.as_ref());
                        ui.master_start = e.dtstart.clone();
                        ui.master_end = e.dtend.clone();
                        ui.recurrence_id = Some(recurrence_id.clone());
                        ui.series_master = Some(master.clone());
                        ui.uid = format!("{}::{}", e.uid, recurrence_id);
                        out.push(ui);
                    }
                    continue;
                }
            }
        }
        out.push(to_ui(e, cal.as_ref()));
    }
    Ok(out)
}

#[tauri::command]
pub fn get_theme() -> ThemeColors {
    theme::load_omarchy_theme()
}

#[tauri::command]
pub fn get_config() -> Result<AppConfig, String> {
    config::load_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_config(cfg: AppConfig) -> Result<(), String> {
    config::save_config(&cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let calendars = state.db.list_calendars().map_err(|e| e.to_string())?;
    let events = build_events(&state.db, &cfg)?;
    let pending = {
        let cals: std::collections::HashMap<i64, CalendarRow> =
            calendars.iter().cloned().map(|c| (c.id, c)).collect();
        let mut seen_uids = std::collections::HashSet::new();
        let mut rows = state.db.pending_invites().map_err(|e| e.to_string())?;
        // Prefer own calendars over shared copies of the same UID
        rows.sort_by_key(|e| {
            let shared = cals
                .get(&e.calendar_id)
                .map(|c| c.href.contains("shared_by") || c.readonly)
                .unwrap_or(true);
            (shared, e.id)
        });
        rows.into_iter()
            .filter(|e| seen_uids.insert(e.uid.clone()))
            .filter(|e| {
                // Drop false pending from duplicate ATTENDEE PARTSTATs
                let attendees: Vec<AttendeeInfo> =
                    serde_json::from_str(&e.attendees_json).unwrap_or_default();
                let account_id = cals
                    .get(&e.calendar_id)
                    .map(|c| c.account_id.as_str())
                    .unwrap_or("");
                let addrs = cfg
                    .accounts
                    .iter()
                    .find(|a| a.id == account_id)
                    .map(|a| a.addresses.as_slice())
                    .unwrap_or(&[]);
                matches!(
                    ics::effective_my_partstat(&attendees, addrs).as_deref(),
                    Some("NEEDS-ACTION")
                )
            })
            .map(|e| {
                let cal = cals.get(&e.calendar_id);
                to_ui(e, cal)
            })
            .collect()
    };
    Ok(AppSnapshot {
        config: cfg,
        calendars,
        events,
        pending_invites: pending,
        theme: theme::load_omarchy_theme(),
        last_sync: state.db.get_meta("last_sync").ok().flatten(),
        last_sync_error: state
            .db
            .get_meta("last_sync_error")
            .ok()
            .flatten()
            .filter(|s| !s.is_empty()),
        default_calendar_id: state.db.get_default_calendar_id().ok().flatten(),
    })
}

#[derive(Deserialize)]
pub struct AddAccountRequest {
    pub display_name: String,
    pub caldav_url: String,
    pub username: String,
    pub password: String,
    pub addresses: Vec<String>,
}

/// Info extracted from an imported .ics file so the UI can prefill the editor.
#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub summary: String,
    pub description: String,
    pub location: String,
    pub dtstart: Option<String>,
    pub dtend: Option<String>,
    pub all_day: bool,
    pub rrule: Option<String>,
    pub alarms: Vec<AlarmInfo>,
    pub attendees: Vec<AttendeeInfo>,
}

#[tauri::command]
pub async fn preview_ics(path: String) -> Result<ImportPreview, String> {
    let p = path;
    let p = if let Some(rest) = p.strip_prefix("file://") {
        percent_encoding::percent_decode_str(rest)
            .decode_utf8_lossy()
            .into_owned()
    } else {
        p
    };
    let raw = std::fs::read_to_string(&p).map_err(|_| format!("could not read {p}"))?;
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let default_tz = cfg.locale.timezone.parse::<chrono_tz::Tz>().ok();
    let parsed = ics::preview_from_ics_with_tz(&raw, default_tz)
        .or_else(|| ics::preview_from_ics(&raw))
        .ok_or_else(|| "not a valid .ics event file".to_string())?;
    Ok(ImportPreview {
        summary: parsed.summary,
        description: parsed.description,
        location: parsed.location,
        dtstart: parsed.dtstart,
        dtend: parsed.dtend,
        all_day: parsed.all_day,
        rrule: parsed.rrule,
        alarms: parsed.alarms,
        attendees: parsed.attendees,
    })
}

#[tauri::command]
pub fn take_pending_imports(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut q = state
        .pending_imports
        .lock()
        .map_err(|_| "import queue lock poisoned".to_string())?;
    Ok(std::mem::take(&mut *q))
}

#[tauri::command]
pub async fn add_account(req: AddAccountRequest) -> Result<AccountConfig, String> {
    // test first
    let names = SyncEngine::test_connection(&req.caldav_url, &req.username, &req.password).await?;
    if names.is_empty() {
        return Err("Connected but no calendars found".into());
    }
    let id = Uuid::new_v4().to_string();
    secrets::set_password(&id, &req.password)?;
    let account = AccountConfig {
        id,
        display_name: req.display_name,
        caldav_url: req.caldav_url,
        username: req.username,
        addresses: req.addresses,
        enabled: true,
    };
    let mut cfg = config::load_config().map_err(|e| e.to_string())?;
    cfg.accounts.push(account.clone());
    config::save_config(&cfg).map_err(|e| e.to_string())?;
    Ok(account)
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, account_id: String) -> Result<(), String> {
    let mut cfg = config::load_config().map_err(|e| e.to_string())?;
    cfg.accounts.retain(|a| a.id != account_id);
    config::save_config(&cfg).map_err(|e| e.to_string())?;
    let _ = secrets::delete_password(&account_id);
    state
        .db
        .remove_calendars_for_account(&account_id)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn test_account(
    caldav_url: String,
    username: String,
    password: String,
) -> Result<Vec<String>, String> {
    SyncEngine::test_connection(&caldav_url, &username, &password).await
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<SyncReport, String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    state.sync.sync_all(&cfg).await
}

#[tauri::command]
pub fn list_calendars(state: State<'_, AppState>) -> Result<Vec<CalendarRow>, String> {
    state.db.list_calendars().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_calendar_visible(
    state: State<'_, AppState>,
    id: i64,
    visible: bool,
) -> Result<(), String> {
    state
        .db
        .set_calendar_visible(id, visible)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_calendar_color(
    state: State<'_, AppState>,
    id: i64,
    color: String,
) -> Result<(), String> {
    state
        .db
        .set_calendar_color(id, &color)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_calendar_subscribed(
    state: State<'_, AppState>,
    id: i64,
    subscribed: bool,
) -> Result<(), String> {
    state
        .db
        .set_calendar_subscribed(id, subscribed)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_default_calendar(state: State<'_, AppState>, id: Option<i64>) -> Result<(), String> {
    state.db.set_default_calendar(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_calendars(
    state: State<'_, AppState>,
    account_id: String,
    ordered_ids: Vec<i64>,
) -> Result<(), String> {
    state
        .db
        .reorder_calendars(&account_id, &ordered_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_events(state: State<'_, AppState>) -> Result<Vec<UiEvent>, String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    build_events(&state.db, &cfg)
}

#[tauri::command]
pub fn search_events(state: State<'_, AppState>, query: String) -> Result<Vec<UiEvent>, String> {
    let calendars = state.db.list_calendars().map_err(|e| e.to_string())?;
    let map: std::collections::HashMap<i64, CalendarRow> =
        calendars.into_iter().map(|c| (c.id, c)).collect();
    let events = state.db.search_events(&query).map_err(|e| e.to_string())?;
    Ok(events
        .into_iter()
        .map(|e| {
            let cal = map.get(&e.calendar_id);
            to_ui(e, cal)
        })
        .collect())
}

#[tauri::command]
pub fn pending_invites(state: State<'_, AppState>) -> Result<Vec<UiEvent>, String> {
    let calendars = state.db.list_calendars().map_err(|e| e.to_string())?;
    let map: std::collections::HashMap<i64, CalendarRow> =
        calendars.into_iter().map(|c| (c.id, c)).collect();
    Ok(state
        .db
        .pending_invites()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|e| {
            let cal = map.get(&e.calendar_id);
            to_ui(e, cal)
        })
        .collect())
}

#[tauri::command]
pub async fn save_event(state: State<'_, AppState>, input: EventInput) -> Result<UiEvent, String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let cal = state
        .db
        .get_calendar(input.calendar_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "calendar not found".to_string())?;
    if cal.readonly {
        return Err("calendar is read-only".into());
    }
    let account = cfg
        .accounts
        .iter()
        .find(|a| a.id == cal.account_id)
        .ok_or_else(|| "account not found".to_string())?
        .clone();

    let existing = if let Some(uid) = &input.uid {
        state.db.get_object_by_uid(uid).ok().flatten()
    } else {
        None
    };

    let (uid, built_ics) = ics::build_ics(&input, existing.as_ref().map(|e| e.uid.as_str()))?;
    let ics_body = if let Some(row) = &existing {
        ics::replace_master_event(&row.raw_ics, &built_ics, input.rrule.is_some())?
    } else {
        built_ics
    };
    let href = input
        .href
        .clone()
        .or_else(|| existing.as_ref().map(|e| e.href.clone()))
        .unwrap_or_else(|| ics::default_href_for_uid(&cal.href, &uid));
    let etag = input
        .etag
        .clone()
        .or_else(|| existing.as_ref().and_then(|e| e.etag.clone()));

    let new_etag = state
        .sync
        .push_event(&account, &cal.href, &href, &ics_body, etag.as_deref())
        .await?;

    let default_tz = input.timezone.parse::<chrono_tz::Tz>().ok();
    let parsed = ics::parse_ics_with_tz(&ics_body, &account.addresses, default_tz)
        .or_else(|| ics::parse_ics(&ics_body, &account.addresses))
        .ok_or_else(|| "failed to parse built ics".to_string())?;
    let attendees_json = serde_json::to_string(&parsed.attendees).unwrap_or_else(|_| "[]".into());
    let alarms_json = serde_json::to_string(&parsed.alarms).unwrap_or_else(|_| "[]".into());

    state
        .db
        .upsert_object(
            cal.id,
            &href,
            new_etag.as_deref().or(etag.as_deref()),
            &uid, // prefer the UID we built/put, not a re-parsed substitute
            &parsed.summary,
            &parsed.description,
            &parsed.location,
            parsed.dtstart.as_deref(),
            parsed.dtend.as_deref(),
            parsed.all_day,
            parsed.rrule.as_deref(),
            &ics_body,
            parsed.status.as_deref(),
            parsed.organizer.as_deref(),
            &attendees_json,
            &alarms_json,
            parsed.my_partstat.as_deref(),
        )
        .map_err(|e| e.to_string())?;

    let row = state
        .db
        .get_object_by_href(cal.id, &href)
        .map_err(|e| e.to_string())?
        .or_else(|| state.db.get_object_by_uid(&uid).ok().flatten())
        .ok_or_else(|| "saved but not found locally".to_string())?;
    Ok(to_ui(row, Some(&cal)))
}

#[derive(Deserialize)]
pub struct SaveOccurrenceRequest {
    pub event_id: i64,
    pub recurrence_id: String,
    pub input: EventInput,
}

#[tauri::command]
pub async fn save_event_occurrence(
    state: State<'_, AppState>,
    req: SaveOccurrenceRequest,
) -> Result<(), String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let row = state
        .db
        .get_object(req.event_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "event not found".to_string())?;
    if row.rrule.is_none() {
        return Err("event is not recurring".to_string());
    }
    if req.input.calendar_id != row.calendar_id {
        return Err("a single occurrence cannot be moved to another calendar".to_string());
    }
    let cal = state
        .db
        .get_calendar(row.calendar_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "calendar not found".to_string())?;
    if cal.readonly {
        return Err("calendar is read-only".into());
    }
    let account = cfg
        .accounts
        .iter()
        .find(|account| account.id == cal.account_id)
        .ok_or_else(|| "account not found".to_string())?
        .clone();
    let default_tz = req.input.timezone.parse::<chrono_tz::Tz>().ok();
    let new_raw =
        ics::upsert_occurrence_override(&row.raw_ics, &req.recurrence_id, &req.input, default_tz)?;
    let etag = req.input.etag.as_deref().or(row.etag.as_deref());
    let new_etag = state
        .sync
        .push_event(&account, &cal.href, &row.href, &new_raw, etag)
        .await?;

    let parsed = ics::parse_ics_with_tz(&new_raw, &account.addresses, default_tz)
        .or_else(|| ics::parse_ics(&new_raw, &account.addresses))
        .ok_or_else(|| "failed to parse event after occurrence edit".to_string())?;
    let attendees_json = serde_json::to_string(&parsed.attendees).unwrap_or_else(|_| "[]".into());
    let alarms_json = serde_json::to_string(&parsed.alarms).unwrap_or_else(|_| "[]".into());
    state
        .db
        .upsert_object(
            cal.id,
            &row.href,
            new_etag.as_deref().or(etag),
            &parsed.uid,
            &parsed.summary,
            &parsed.description,
            &parsed.location,
            parsed.dtstart.as_deref(),
            parsed.dtend.as_deref(),
            parsed.all_day,
            parsed.rrule.as_deref(),
            &new_raw,
            parsed.status.as_deref(),
            parsed.organizer.as_deref(),
            &attendees_json,
            &alarms_json,
            parsed.my_partstat.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_event(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let row = state
        .db
        .get_object(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "event not found".to_string())?;
    let cal = state
        .db
        .get_calendar(row.calendar_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "calendar not found".to_string())?;
    if cal.readonly {
        return Err("calendar is read-only".into());
    }
    let account = cfg
        .accounts
        .iter()
        .find(|a| a.id == cal.account_id)
        .ok_or_else(|| "account not found".to_string())?;

    state
        .sync
        .delete_remote(account, &row.href, row.etag.as_deref())
        .await?;
    state
        .db
        .delete_object_by_id(id)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Deserialize)]
pub struct DeleteOccurrenceRequest {
    pub id: i64,
    pub occurrence_start: String, // RFC3339 for timed, YYYY-MM-DD for all-day
    pub mode: String,             // "single" | "future" | "all"
}

#[tauri::command]
pub async fn delete_event_occurrence(
    state: State<'_, AppState>,
    req: DeleteOccurrenceRequest,
) -> Result<(), String> {
    if req.mode == "all" {
        return delete_event(state, req.id).await;
    }
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let row = state
        .db
        .get_object(req.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "event not found".to_string())?;
    if row.rrule.is_none() {
        // Not recurring — fallback to full delete
        return delete_event(state, req.id).await;
    }
    let cal = state
        .db
        .get_calendar(row.calendar_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "calendar not found".to_string())?;
    if cal.readonly {
        return Err("calendar is read-only".into());
    }
    let account = cfg
        .accounts
        .iter()
        .find(|a| a.id == cal.account_id)
        .ok_or_else(|| "account not found".to_string())?
        .clone();

    // Parse occurrence instant
    let parsed_occ = if req.occurrence_start.contains('T') {
        chrono::DateTime::parse_from_rfc3339(&req.occurrence_start)
            .map(|d| d.with_timezone(&chrono::Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(&req.occurrence_start, "%Y-%m-%dT%H:%M:%S")
                    .map(|n| {
                        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(n, chrono::Utc)
                    })
            })
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(&req.occurrence_start, "%Y-%m-%d %H:%M:%S")
                    .map(|n| {
                        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(n, chrono::Utc)
                    })
            })
            .map_err(|_| format!("cannot parse occurrence_start {}", req.occurrence_start))?
    } else {
        // All-day date
        let d = chrono::NaiveDate::parse_from_str(
            &req.occurrence_start[..10.min(req.occurrence_start.len())],
            "%Y-%m-%d",
        )
        .map_err(|_| format!("cannot parse occurrence date {}", req.occurrence_start))?;
        let ndt = d.and_hms_opt(0, 0, 0).unwrap();
        // Use calendar tz for all-day wall
        let tz = row
            .raw_ics
            .lines()
            .find(|l| l.to_uppercase().contains("DTSTART"))
            .and_then(|_| {
                ics::extract_wall_dt_and_tz(&row.raw_ics, "DTSTART").and_then(|(_, tz, _)| tz)
            })
            .or_else(|| cfg.locale.timezone.parse::<chrono_tz::Tz>().ok());
        if let Some(tz) = tz {
            if let Some(ldt) = tz
                .from_local_datetime(&ndt)
                .single()
                .or_else(|| tz.from_local_datetime(&ndt).earliest())
            {
                ldt.with_timezone(&chrono::Utc)
            } else {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc)
            }
        } else {
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc)
        }
    };

    let default_tz = cfg.locale.timezone.parse::<chrono_tz::Tz>().ok();
    let new_raw = match req.mode.as_str() {
        "single" => {
            let without_override =
                ics::remove_occurrence_override(&row.raw_ics, &req.occurrence_start, default_tz);
            // Detect DTSTART tz for EXDATE form
            let detected_tz =
                ics::extract_wall_dt_and_tz(&row.raw_ics, "DTSTART").and_then(|(_, tz, _)| tz);
            let dtstart_tz = if row.all_day {
                detected_tz.or(default_tz)
            } else {
                detected_tz
            };
            ics::inject_exdate(&without_override, parsed_occ, dtstart_tz)
        }
        "future" => {
            // UNTIL = occurrence - 1s (excludes this and future)
            let until_utc = parsed_occ - chrono::Duration::seconds(1);
            let dtstart_info = ics::extract_wall_dt_and_tz(&row.raw_ics, "DTSTART");
            let until_tz = dtstart_info
                .and_then(|(_, tz, _)| tz)
                .or_else(|| cfg.locale.timezone.parse::<chrono_tz::Tz>().ok());
            // For all-day we want date-only UNTIL (occurrence date -1)
            if row.all_day {
                let occurrence_date = chrono::NaiveDate::parse_from_str(
                    &req.occurrence_start[..10.min(req.occurrence_start.len())],
                    "%Y-%m-%d",
                )
                .map_err(|_| format!("cannot parse occurrence date {}", req.occurrence_start))?;
                let until_date = occurrence_date - chrono::Duration::days(1);
                let until_wall = until_date.and_hms_opt(0, 0, 0).unwrap();
                let truncated = ics::truncate_rrule_until(&row.raw_ics, until_wall, None, true);
                ics::remove_future_overrides(&truncated, &req.occurrence_start, default_tz)
            } else if let Some(tz) = until_tz {
                let until_wall = until_utc.with_timezone(&tz).naive_local();
                let truncated =
                    ics::truncate_rrule_until(&row.raw_ics, until_wall, Some(tz), false);
                ics::remove_future_overrides(&truncated, &req.occurrence_start, default_tz)
            } else {
                let until_wall = until_utc.naive_utc();
                let truncated = ics::truncate_rrule_until(&row.raw_ics, until_wall, None, false);
                ics::remove_future_overrides(&truncated, &req.occurrence_start, default_tz)
            }
        }
        _ => return Err(format!("unknown mode {}", req.mode)),
    };

    // If future truncate results in UNTIL before DTSTART, treat as delete entire series
    if req.mode == "future" {
        if row.all_day {
            let master_date = row.dtstart.as_deref().and_then(|value| {
                chrono::NaiveDate::parse_from_str(&value[..10.min(value.len())], "%Y-%m-%d").ok()
            });
            let occurrence_date = chrono::NaiveDate::parse_from_str(
                &req.occurrence_start[..10.min(req.occurrence_start.len())],
                "%Y-%m-%d",
            )
            .ok();
            if matches!((master_date, occurrence_date), (Some(master), Some(occurrence)) if occurrence <= master)
            {
                return delete_event(state, req.id).await;
            }
        }
        // Quick check: parse DTSTART wall and compare
        if let Some((wall_start, tz, _)) = ics::extract_wall_dt_and_tz(&new_raw, "DTSTART") {
            // Extract UNTIL
            let until_opt = {
                let mut found = None;
                for line in new_raw.lines() {
                    let up = line.to_uppercase();
                    if up.starts_with("RRULE") {
                        if let Some(idx) = up.find("UNTIL=") {
                            let after = &line[idx + 6..];
                            let end = after.find(';').unwrap_or(after.len());
                            found = Some(after[..end].trim());
                        }
                    }
                }
                found.map(|s| s.to_string())
            };
            if let Some(until_s) = until_opt {
                let until_utc = if until_s.ends_with('Z') {
                    chrono::DateTime::parse_from_str(&until_s, "%Y%m%dT%H%M%SZ")
                        .ok()
                        .map(|d| d.with_timezone(&chrono::Utc))
                } else if let Ok(ndt) =
                    chrono::NaiveDateTime::parse_from_str(&until_s, "%Y%m%dT%H%M%S")
                {
                    if let Some(tz) = tz {
                        tz.from_local_datetime(&ndt)
                            .single()
                            .or_else(|| tz.from_local_datetime(&ndt).earliest())
                            .map(|ldt| ldt.with_timezone(&chrono::Utc))
                    } else {
                        Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                            ndt,
                            chrono::Utc,
                        ))
                    }
                } else if let Ok(d) = chrono::NaiveDate::parse_from_str(&until_s, "%Y%m%d") {
                    let ndt = d.and_hms_opt(0, 0, 0).unwrap();
                    if let Some(tz) = tz {
                        tz.from_local_datetime(&ndt)
                            .single()
                            .or_else(|| tz.from_local_datetime(&ndt).earliest())
                            .map(|ldt| ldt.with_timezone(&chrono::Utc))
                    } else {
                        Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                            ndt,
                            chrono::Utc,
                        ))
                    }
                } else {
                    None
                };
                if let (Some(u), Some(start_utc)) = (until_utc, {
                    if let Some(tz) = tz {
                        tz.from_local_datetime(&wall_start)
                            .single()
                            .or_else(|| tz.from_local_datetime(&wall_start).earliest())
                            .map(|ldt| ldt.with_timezone(&chrono::Utc))
                    } else {
                        Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                            wall_start,
                            chrono::Utc,
                        ))
                    }
                }) {
                    if u < start_utc {
                        // No occurrences left — delete entire series
                        return delete_event(state, req.id).await;
                    }
                }
            }
        }
    }

    let new_etag = state
        .sync
        .push_event(
            &account,
            &cal.href,
            &row.href,
            &new_raw,
            row.etag.as_deref(),
        )
        .await?;

    let tz_hint = ics::extract_wall_dt_and_tz(&new_raw, "DTSTART")
        .and_then(|(_, tz, _)| tz)
        .or_else(|| cfg.locale.timezone.parse::<chrono_tz::Tz>().ok());
    let parsed = ics::parse_ics_with_tz(&new_raw, &account.addresses, tz_hint)
        .or_else(|| ics::parse_ics(&new_raw, &account.addresses))
        .ok_or_else(|| "parse failed after exdate/truncate".to_string())?;
    let attendees_json = serde_json::to_string(&parsed.attendees).unwrap_or_else(|_| "[]".into());
    let alarms_json = serde_json::to_string(&parsed.alarms).unwrap_or_else(|_| "[]".into());
    state
        .db
        .upsert_object(
            cal.id,
            &row.href,
            new_etag.as_deref().or(row.etag.as_deref()),
            &parsed.uid,
            &parsed.summary,
            &parsed.description,
            &parsed.location,
            parsed.dtstart.as_deref(),
            parsed.dtend.as_deref(),
            parsed.all_day,
            parsed.rrule.as_deref(),
            &new_raw,
            parsed.status.as_deref(),
            parsed.organizer.as_deref(),
            &attendees_json,
            &alarms_json,
            parsed.my_partstat.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Deserialize)]
pub struct RsvpRequest {
    pub event_id: i64,
    pub partstat: String, // ACCEPTED, DECLINED, TENTATIVE
}

#[derive(Deserialize)]
pub struct BulkRsvpRequest {
    pub event_ids: Vec<i64>,
    pub partstat: String,
}

#[derive(Serialize)]
pub struct BulkRsvpResult {
    pub ok: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

async fn apply_rsvp(state: &AppState, event_id: i64, partstat: &str) -> Result<UiEvent, String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let initial = state
        .db
        .get_object(event_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "event not found".to_string())?;
    let initial_cal = state
        .db
        .get_calendar(initial.calendar_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "calendar not found".to_string())?;

    // Prefer RSVP on own calendar copy; shared calendars often 404/403 on PUT
    let mut target_id = event_id;
    let shared = initial_cal.href.contains("shared_by") || initial_cal.readonly;
    if shared {
        if let Some(own) = state
            .db
            .find_own_writable_object_by_uid(&initial.uid, &initial_cal.account_id)
            .map_err(|e| e.to_string())?
        {
            target_id = own.id;
        }
    }

    match apply_rsvp_on(state, &cfg, target_id, partstat).await {
        Ok(ui) => Ok(ui),
        Err(e) => {
            let msg = e.to_string();
            let stale = msg.contains("404") || msg.contains("NotFound");
            let forbidden = msg.contains("403") || msg.contains("Forbidden");
            if target_id == event_id && (stale || forbidden) {
                if let Some(own) = state
                    .db
                    .find_own_writable_object_by_uid(&initial.uid, &initial_cal.account_id)
                    .map_err(|e| e.to_string())?
                {
                    if own.id != event_id {
                        return apply_rsvp_on(state, &cfg, own.id, partstat).await;
                    }
                }
            }
            if stale {
                let mut ui = to_ui(initial.clone(), Some(&initial_cal));
                ui.my_partstat = Some(partstat.to_string());
                let _ = state.db.delete_object_by_id(target_id);
                if target_id != event_id {
                    let _ = state.db.delete_object_by_id(event_id);
                }
                // Treat as handled so single-card RSVP buttons refresh cleanly
                return Ok(ui);
            }
            Err(msg)
        }
    }
}

async fn apply_rsvp_on(
    state: &AppState,
    cfg: &AppConfig,
    event_id: i64,
    partstat: &str,
) -> Result<UiEvent, String> {
    let row = state
        .db
        .get_object(event_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "event not found".to_string())?;
    let cal = state
        .db
        .get_calendar(row.calendar_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "calendar not found".to_string())?;
    let account = cfg
        .accounts
        .iter()
        .find(|a| a.id == cal.account_id)
        .ok_or_else(|| "account not found".to_string())?
        .clone();

    let new_ics = ics::set_partstat_in_ics(&row.raw_ics, &account.addresses, partstat);
    let new_etag = state
        .sync
        .push_event(
            &account,
            &cal.href,
            &row.href,
            &new_ics,
            row.etag.as_deref(),
        )
        .await?;

    let tz_hint = {
        // try to extract TZID from the ICS, else use config timezone
        let cfg_tz = cfg.locale.timezone.parse::<chrono_tz::Tz>().ok();
        if let Some((_, Some(tz), _)) = ics::extract_wall_dt_and_tz(&new_ics, "DTSTART") {
            Some(tz)
        } else {
            cfg_tz
        }
    };
    let parsed = ics::parse_ics_with_tz(&new_ics, &account.addresses, tz_hint)
        .or_else(|| ics::parse_ics(&new_ics, &account.addresses))
        .ok_or_else(|| "parse failed".to_string())?;
    let attendees_json = serde_json::to_string(&parsed.attendees).unwrap_or_else(|_| "[]".into());
    let alarms_json = serde_json::to_string(&parsed.alarms).unwrap_or_else(|_| "[]".into());

    state
        .db
        .upsert_object(
            cal.id,
            &row.href,
            new_etag.as_deref().or(row.etag.as_deref()),
            &parsed.uid,
            &parsed.summary,
            &parsed.description,
            &parsed.location,
            parsed.dtstart.as_deref(),
            parsed.dtend.as_deref(),
            parsed.all_day,
            parsed.rrule.as_deref(),
            &new_ics,
            parsed.status.as_deref(),
            parsed.organizer.as_deref(),
            &attendees_json,
            &alarms_json,
            parsed.my_partstat.as_deref(),
        )
        .map_err(|e| e.to_string())?;

    let updated = state
        .db
        .get_object(event_id)
        .map_err(|e| e.to_string())?
        .or_else(|| state.db.get_object_by_uid(&parsed.uid).ok().flatten())
        .ok_or_else(|| "missing after rsvp".to_string())?;
    Ok(to_ui(updated, Some(&cal)))
}

fn normalize_partstat(partstat: &str) -> Result<String, String> {
    let partstat = partstat.to_uppercase();
    if !matches!(
        partstat.as_str(),
        "ACCEPTED" | "DECLINED" | "TENTATIVE" | "NEEDS-ACTION"
    ) {
        return Err("invalid partstat".into());
    }
    Ok(partstat)
}

#[tauri::command]
pub async fn respond_invite(
    state: State<'_, AppState>,
    req: RsvpRequest,
) -> Result<UiEvent, String> {
    let partstat = normalize_partstat(&req.partstat)?;
    apply_rsvp(&state, req.event_id, &partstat).await
}

#[tauri::command]
pub async fn respond_invites_bulk(
    state: State<'_, AppState>,
    req: BulkRsvpRequest,
) -> Result<BulkRsvpResult, String> {
    let partstat = normalize_partstat(&req.partstat)?;
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut errors = Vec::new();
    for event_id in req.event_ids {
        match apply_rsvp(&state, event_id, &partstat).await {
            Ok(_) => ok += 1,
            Err(e) => {
                failed += 1;
                if errors.len() < 20 {
                    errors.push(format!("#{event_id}: {e}"));
                }
            }
        }
    }
    Ok(BulkRsvpResult { ok, failed, errors })
}

#[tauri::command]
pub fn next_event(state: State<'_, AppState>) -> Result<Option<UiEvent>, String> {
    let now = chrono::Utc::now();
    let row = state.db.next_event_after(&now).map_err(|e| e.to_string())?;
    match row {
        Some(e) => {
            let cal = state.db.get_calendar(e.calendar_id).ok().flatten();
            Ok(Some(to_ui(e, cal.as_ref())))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn freebusy(
    _state: State<'_, AppState>,
    account_id: String,
    start: String,
    end: String,
    attendees: Vec<String>,
) -> Result<String, String> {
    let cfg = config::load_config().map_err(|e| e.to_string())?;
    let account = cfg
        .accounts
        .iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| "account not found".to_string())?;
    let password = secrets::get_password(&account.id)?;
    let client =
        crate::caldav::CalDavClient::new(&account.caldav_url, &account.username, &password)
            .map_err(|e| e.to_string())?;
    let principal = client
        .discover_principal()
        .await
        .map_err(|e| e.to_string())?;
    let home = client
        .discover_calendar_home(&principal)
        .await
        .map_err(|e| e.to_string())?;
    client
        .freebusy(&home, &start, &end, &attendees)
        .await
        .map_err(|e| e.to_string())
}
