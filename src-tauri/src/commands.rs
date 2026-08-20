use crate::config::{self, AccountConfig, AppConfig};
use crate::db::{AlarmInfo, AttendeeInfo, CalendarRow, Db, EventRow};
use crate::ics::{self, EventInput};
use crate::secrets;
use crate::sync::{SyncEngine, SyncReport};
use crate::theme::{self, ThemeColors};
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
}

fn to_ui(ev: EventRow, cal: Option<&CalendarRow>) -> UiEvent {
    let attendees: Vec<AttendeeInfo> =
        serde_json::from_str(&ev.attendees_json).unwrap_or_default();
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
        start: ev.dtstart,
        end: ev.dtend,
        all_day: ev.all_day,
        rrule: ev.rrule,
        color: cal.map(|c| c.color.clone()).unwrap_or_else(|| "#829dd4".into()),
        calendar_name: cal
            .map(|c| c.displayname.clone())
            .unwrap_or_default(),
        status: ev.status,
        organizer: ev.organizer,
        attendees,
        alarms,
        my_partstat: ev.my_partstat,
        readonly: cal.map(|c| c.readonly).unwrap_or(false),
    }
}

fn build_events(db: &Db) -> Result<Vec<UiEvent>, String> {
    let calendars = db.list_calendars().map_err(|e| e.to_string())?;
    let map: std::collections::HashMap<i64, CalendarRow> =
        calendars.into_iter().map(|c| (c.id, c)).collect();
    let events = db.list_events(true).map_err(|e| e.to_string())?;
    let range_start = chrono::Utc::now() - chrono::Duration::days(60);
    let range_end = chrono::Utc::now() + chrono::Duration::days(400);
    let mut out = Vec::new();
    for e in events {
        let cal = map.get(&e.calendar_id).cloned();
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
                    for occ in occurrences {
                        let mut ui = to_ui(
                            EventRow {
                                dtstart: Some(occ.to_rfc3339()),
                                dtend: Some((occ + duration).to_rfc3339()),
                                ..e.clone()
                            },
                            cal.as_ref(),
                        );
                        // unique id for FC: keep base id, encode occurrence in uid display
                        ui.uid = format!("{}::{}", e.uid, occ.timestamp());
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
    let events = build_events(&state.db)?;
    let pending = {
        let cals: std::collections::HashMap<i64, CalendarRow> =
            calendars.iter().cloned().map(|c| (c.id, c)).collect();
        let mut seen_uids = std::collections::HashSet::new();
        let mut rows = state
            .db
            .pending_invites()
            .map_err(|e| e.to_string())?;
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
    let names =
        SyncEngine::test_connection(&req.caldav_url, &req.username, &req.password).await?;
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
pub fn set_calendar_color(state: State<'_, AppState>, id: i64, color: String) -> Result<(), String> {
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
pub fn set_default_calendar(
    state: State<'_, AppState>,
    id: Option<i64>,
) -> Result<(), String> {
    state
        .db
        .set_default_calendar(id)
        .map_err(|e| e.to_string())
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
    build_events(&state.db)
}

#[tauri::command]
pub fn search_events(state: State<'_, AppState>, query: String) -> Result<Vec<UiEvent>, String> {
    let calendars = state.db.list_calendars().map_err(|e| e.to_string())?;
    let map: std::collections::HashMap<i64, CalendarRow> =
        calendars.into_iter().map(|c| (c.id, c)).collect();
    let events = state
        .db
        .search_events(&query)
        .map_err(|e| e.to_string())?;
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

    let (uid, ics_body) =
        ics::build_ics(&input, existing.as_ref().map(|e| e.uid.as_str()))?;
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

async fn apply_rsvp(
    state: &AppState,
    event_id: i64,
    partstat: &str,
) -> Result<UiEvent, String> {
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
pub async fn respond_invite(state: State<'_, AppState>, req: RsvpRequest) -> Result<UiEvent, String> {
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
    let row = state
        .db
        .next_event_after(&now)
        .map_err(|e| e.to_string())?;
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
    let client = crate::caldav::CalDavClient::new(&account.caldav_url, &account.username, &password)
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
