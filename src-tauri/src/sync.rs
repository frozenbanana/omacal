use crate::caldav::CalDavClient;
use crate::config::{AccountConfig, AppConfig};
use crate::db::Db;
use crate::ics;
use crate::secrets;
use std::sync::Arc;

pub struct SyncEngine {
    pub db: Arc<Db>,
}

impl SyncEngine {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn sync_all(&self, cfg: &AppConfig) -> Result<SyncReport, String> {
        // One-time repair: VTIMEZONE RRULEs were stored as event rrules; PARTSTAT dupes
        if self
            .db
            .get_meta("ics_vevent_parse_v1")
            .ok()
            .flatten()
            .as_deref()
            != Some("1")
        {
            let mut addrs = std::collections::HashMap::new();
            for a in &cfg.accounts {
                addrs.insert(a.id.clone(), a.addresses.clone());
            }
            match self.db.repair_derived_ics_fields(&addrs) {
                Ok(n) => {
                    log::info!("repaired derived ICS fields on {n} events");
                    let _ = self.db.set_meta("ics_vevent_parse_v1", "1");
                }
                Err(e) => log::warn!("ICS field repair failed: {e}"),
            }
        }

        let mut report = SyncReport::default();
        for account in cfg.accounts.iter().filter(|a| a.enabled) {
            match self.sync_account(account).await {
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
        let _ = self.db.set_meta(
            "last_sync",
            &chrono::Utc::now().to_rfc3339(),
        );
        if !report.errors.is_empty() {
            let _ = self.db.set_meta("last_sync_error", &report.errors.join("; "));
        } else {
            let _ = self.db.set_meta("last_sync_error", "");
        }
        Ok(report)
    }

    pub async fn sync_account(&self, account: &AccountConfig) -> Result<SyncReport, String> {
        let password = secrets::get_password(&account.id)?;
        let client = CalDavClient::new(&account.caldav_url, &account.username, &password)
            .map_err(|e| e.to_string())?;

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
            let color = remote
                .color
                .clone()
                .unwrap_or_else(|| "#829dd4".into());
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
                if let Some(parsed) = ics::parse_ics(raw, &account.addresses) {
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
                .set_calendar_sync_state(
                    cal_id,
                    new_token.as_deref().or(token.as_deref()),
                    ctag,
                )
                .map_err(|e| e.to_string())?;
        }

        Ok(report)
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
        let client = CalDavClient::new(&account.caldav_url, &account.username, &password)
            .map_err(|e| e.to_string())?;
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
        let client = CalDavClient::new(&account.caldav_url, &account.username, &password)
            .map_err(|e| e.to_string())?;
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
        let client =
            CalDavClient::new(url, username, password).map_err(|e| e.to_string())?;
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
    pub errors: Vec<String>,
}
