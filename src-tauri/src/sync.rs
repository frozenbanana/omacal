use crate::caldav::CalDavClient;
use crate::config::{AccountConfig, AppConfig};
use crate::db::Db;
use crate::ics;
use crate::secrets;
use std::sync::{Arc, OnceLock};

/// Heuristic: does a CalDAV error string indicate a network/connectivity
/// failure (as opposed to an HTTP-level rejection like 401/403/412)?
/// Used to decide whether to queue a write in the offline outbox.
pub fn is_network_error(e: &str) -> bool {
    let low = e.to_lowercase();
    // Server replies with an HTTP status are always "PUT failed: NNN .." / "DELETE failed: ..";
    // reqwest send failures mention "error sending request".
    if low.contains("put failed") || low.contains("delete failed") || low.contains("unauthorized") {
        return false;
    }
    const HINTS: [&str; 12] = [
        "error sending request",
        "timed out",
        "timeout",
        "connection",
        "dns error",
        "temporary failure",
        "unreachable",
        "refused",
        "reset by peer",
        "broken pipe",
        "tls",
        "certificate",
    ];
    HINTS.iter().any(|h| low.contains(h))
}

pub struct SyncEngine {
    pub db: Arc<Db>,
    http: OnceLock<reqwest::Client>,
}

impl SyncEngine {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            http: OnceLock::new(),
        }
    }

    fn client_for(&self, account: &AccountConfig, password: &str) -> Result<CalDavClient, String> {
        if self.http.get().is_none() {
            let http = CalDavClient::build_http_client().map_err(|e| e.to_string())?;
            let _ = self.http.set(http);
        }
        let http = self
            .http
            .get()
            .expect("HTTP client initialized above")
            .clone();
        Ok(CalDavClient::with_http(
            http,
            &account.caldav_url,
            &account.username,
            password,
        ))
    }

    pub async fn sync_all(&self, cfg: &AppConfig) -> Result<SyncReport, String> {
        // One-time repair: VTIMEZONE RRULEs + floating TZ + dst-safe rrule
        for ver in ["ics_vevent_parse_v1", "ics_tz_fix_v2"] {
            if self.db.get_meta(ver).ok().flatten().as_deref() != Some("1") {
                let mut addrs = std::collections::HashMap::new();
                for a in &cfg.accounts {
                    addrs.insert(a.id.clone(), a.addresses.clone());
                }
                let default_tz = cfg.locale.timezone.parse::<chrono_tz::Tz>().ok();
                match self
                    .db
                    .repair_derived_ics_fields_with_tz(&addrs, default_tz)
                {
                    Ok(n) => {
                        log::info!("repaired {ver} on {n} events");
                        let _ = self.db.set_meta(ver, "1");
                    }
                    Err(e) => log::warn!("ICS repair {ver} failed: {e}"),
                }
            }
        }
        // Legacy single-ver path migration: ensure v1 is marked after v2
        if self
            .db
            .get_meta("ics_vevent_parse_v1")
            .ok()
            .flatten()
            .as_deref()
            != Some("1")
        {
            let _ = self.db.set_meta("ics_vevent_parse_v1", "1");
        }

        let mut report = SyncReport::default();
        // Replay queued offline writes first, so the pull phase below
        // sees post-replay server state (deletes, renames, ...).
        if let Err(e) = self.flush_outbox(cfg, &mut report).await {
            report.errors.push(format!("outbox flush failed: {e}"));
        }
        for account in cfg.accounts.iter().filter(|a| a.enabled) {
            match self.sync_account(account, cfg).await {
                Ok(r) => {
                    report.calendars += r.calendars;
                    report.objects += r.objects;
                    report.deleted += r.deleted;
                }
                Err(e) => {
                    report.errors.push(format!("{}: {e}", account.display_name));
                }
            }
        }
        let _ = self
            .db
            .set_meta("last_sync", &chrono::Utc::now().to_rfc3339());
        if !report.errors.is_empty() {
            let _ = self
                .db
                .set_meta("last_sync_error", &report.errors.join("; "));
        } else {
            let _ = self.db.set_meta("last_sync_error", "");
        }
        Ok(report)
    }

    pub async fn sync_account(
        &self,
        account: &AccountConfig,
        cfg: &AppConfig,
    ) -> Result<SyncReport, String> {
        let password = secrets::get_password(&account.id)?;
        let client = self.client_for(account, &password)?;

        let principal = client
            .discover_principal()
            .await
            .map_err(|e| e.to_string())?;
        let home = client
            .discover_calendar_home(&principal)
            .await
            .map_err(|e| e.to_string())?;
        let remotes = client
            .list_calendars(&home)
            .await
            .map_err(|e| e.to_string())?;

        let mut report = SyncReport {
            calendars: remotes.len(),
            ..Default::default()
        };

        for remote in remotes {
            let color = remote.color.clone().unwrap_or_else(|| "#829dd4".into());
            let cal_id = self
                .db
                .upsert_calendar(
                    &account.id,
                    &remote.href,
                    &remote.displayname,
                    &color,
                    remote.readonly,
                )
                .map_err(|e| e.to_string())?;

            let local = self.db.get_calendar(cal_id).map_err(|e| e.to_string())?;
            // Keep row/prefs for unsubscribed calendars but do not fetch objects
            if local.as_ref().is_some_and(|c| !c.subscribed) {
                continue;
            }
            let token = local.as_ref().and_then(|c| c.sync_token.clone());

            let (objects, new_token) = client
                .sync_collection(&remote.href, token.as_deref())
                .await
                .map_err(|e| e.to_string())?;

            for obj in objects {
                if obj.data.is_none() {
                    // deleted
                    self.db
                        .delete_object_by_href(cal_id, &obj.href)
                        .map_err(|e| e.to_string())?;
                    report.deleted += 1;
                    continue;
                }
                let raw = obj.data.as_deref().unwrap();
                let default_tz = cfg
                    .locale
                    .timezone
                    .parse::<chrono_tz::Tz>()
                    .ok()
                    .or_else(|| Some(ics::default_tz()));
                if let Some(parsed) = ics::parse_ics_with_tz(raw, &account.addresses, default_tz) {
                    let attendees_json =
                        serde_json::to_string(&parsed.attendees).unwrap_or_else(|_| "[]".into());
                    let alarms_json =
                        serde_json::to_string(&parsed.alarms).unwrap_or_else(|_| "[]".into());
                    self.db
                        .upsert_object(
                            cal_id,
                            &obj.href,
                            obj.etag.as_deref(),
                            &parsed.uid,
                            &parsed.summary,
                            &parsed.description,
                            &parsed.location,
                            parsed.dtstart.as_deref(),
                            parsed.dtend.as_deref(),
                            parsed.all_day,
                            parsed.rrule.as_deref(),
                            raw,
                            parsed.status.as_deref(),
                            parsed.organizer.as_deref(),
                            &attendees_json,
                            &alarms_json,
                            parsed.my_partstat.as_deref(),
                        )
                        .map_err(|e| e.to_string())?;
                    report.objects += 1;
                }
            }

            let ctag = remote.ctag.as_deref();
            self.db
                .set_calendar_sync_state(cal_id, new_token.as_deref().or(token.as_deref()), ctag)
                .map_err(|e| e.to_string())?;
        }

        Ok(report)
    }

    /// Replay queued offline writes (PUT/DELETE) against their calendars.
    /// Runs before the pull phase of each sync so queued deletes are
    /// reflected before sync-collection re-downloads unchanged objects.
    pub async fn flush_outbox(
        &self,
        cfg: &AppConfig,
        report: &mut SyncReport,
    ) -> Result<usize, String> {
        let rows = self.db.list_outbox().map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(0);
        }
        let mut replayed = 0usize;
        let mut offline_stopped = false;
        for row in rows {
            if offline_stopped {
                continue; // network is down — leave remaining rows queued
            }
            let cal = match self.db.get_calendar(row.calendar_id).ok().flatten() {
                Some(c) => c,
                None => {
                    // Calendar vanished locally; discard the stale op.
                    let _ = self.db.remove_outbox(row.id);
                    continue;
                }
            };
            let Some(account) = cfg.accounts.iter().find(|a| a.id == cal.account_id) else {
                let _ = self.db.remove_outbox(row.id);
                continue;
            };
            if !account.enabled {
                continue;
            }
            let href = match &row.href {
                Some(h) => h.clone(),
                None => {
                    let _ = self.db.remove_outbox(row.id);
                    continue;
                }
            };

            let result = match row.op.as_str() {
                "put" => {
                    let raw = match &row.raw_ics {
                        Some(r) => r.clone(),
                        None => {
                            let _ = self.db.remove_outbox(row.id);
                            continue;
                        }
                    };
                    match self
                        .push_event(account, &cal.href, &href, &raw, row.etag.as_deref())
                        .await
                    {
                        Ok(new_etag) => {
                            // Keep the local row in sync with the server etag.
                            if let Some(existing) =
                                self.db.get_object_by_href(cal.id, &href).ok().flatten()
                            {
                                let _ = self.db.set_object_etag(existing.id, new_etag.as_deref());
                            }
                            Ok(())
                        }
                        Err(e) => {
                            // Stale If-Match etag: retry once without it (last-write-wins).
                            if e.contains("412") {
                                self.push_event(account, &cal.href, &href, &raw, None)
                                    .await
                                    .map(|_: Option<String>| ())
                            } else {
                                Err(e)
                            }
                        }
                    }
                }
                "delete" => {
                    self.delete_remote(account, &href, row.etag.as_deref())
                        .await
                }
                other => {
                    log::warn!("unknown outbox op '{other}' — dropping row {}", row.id);
                    let _ = self.db.remove_outbox(row.id);
                    continue;
                }
            };

            match result {
                Ok(()) => {
                    let _ = self.db.remove_outbox(row.id);
                    replayed += 1;
                }
                Err(e) => {
                    if is_network_error(&e) {
                        offline_stopped = true;
                        let _ = self.db.bump_outbox_attempts(row.id, &e);
                        continue;
                    }
                    // Permanent failures (4xx): retries won't help.
                    // Give up after 5 attempts so the queue can't wedge forever.
                    match self.db.bump_outbox_attempts(row.id, &e) {
                        Ok(n) if n >= 5 => {
                            let _ = self.db.remove_outbox(row.id);
                            report.errors.push(format!(
                                "dropped queued {} for {} after {n} attempts: {e}",
                                row.op, row.uid
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        report.flushed = replayed;
        Ok(replayed)
    }

    pub async fn push_event(
        &self,
        account: &AccountConfig,
        calendar_href: &str,
        href: &str,
        ics_body: &str,
        etag: Option<&str>,
    ) -> Result<Option<String>, String> {
        let password = secrets::get_password(&account.id)?;
        let client = self.client_for(account, &password)?;
        // Ensure href is under calendar
        let full_href = if href.starts_with('/') || href.starts_with("http") {
            href.to_string()
        } else {
            format!(
                "{}/{}",
                calendar_href.trim_end_matches('/'),
                href.trim_start_matches('/')
            )
        };
        client
            .put_object(&full_href, ics_body, etag)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete_remote(
        &self,
        account: &AccountConfig,
        href: &str,
        etag: Option<&str>,
    ) -> Result<(), String> {
        let password = secrets::get_password(&account.id)?;
        let client = self.client_for(account, &password)?;
        client
            .delete_object(href, etag)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn test_connection(
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<Vec<String>, String> {
        let client = CalDavClient::new(url, username, password).map_err(|e| e.to_string())?;
        let principal = client
            .discover_principal()
            .await
            .map_err(|e| e.to_string())?;
        let home = client
            .discover_calendar_home(&principal)
            .await
            .map_err(|e| e.to_string())?;
        let cals = client
            .list_calendars(&home)
            .await
            .map_err(|e| e.to_string())?;
        Ok(cals.into_iter().map(|c| c.displayname).collect())
    }
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SyncReport {
    pub calendars: usize,
    pub objects: usize,
    pub deleted: usize,
    pub flushed: usize,
    pub errors: Vec<String>,
}
