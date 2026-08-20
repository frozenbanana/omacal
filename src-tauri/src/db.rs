use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarRow {
    pub id: i64,
    pub account_id: String,
    pub href: String,
    pub displayname: String,
    pub color: String,
    pub sync_token: Option<String>,
    pub ctag: Option<String>,
    pub visible: bool,
    pub readonly: bool,
    pub subscribed: bool,
    pub sort_order: i64,
}

pub const META_DEFAULT_CALENDAR_ID: &str = "default_calendar_id";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: i64,
    pub calendar_id: i64,
    pub href: String,
    pub etag: Option<String>,
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub dtstart: Option<String>,
    pub dtend: Option<String>,
    pub all_day: bool,
    pub rrule: Option<String>,
    pub raw_ics: String,
    pub status: Option<String>,
    pub organizer: Option<String>,
    pub attendees_json: String,
    pub alarms_json: String,
    pub my_partstat: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendeeInfo {
    pub email: String,
    pub cn: Option<String>,
    pub partstat: Option<String>,
    pub role: Option<String>,
    pub rsvp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmInfo {
    pub trigger: String,
    pub description: Option<String>,
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS calendars (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL,
                href TEXT NOT NULL,
                displayname TEXT NOT NULL DEFAULT '',
                color TEXT NOT NULL DEFAULT '#829dd4',
                sync_token TEXT,
                ctag TEXT,
                visible INTEGER NOT NULL DEFAULT 1,
                readonly INTEGER NOT NULL DEFAULT 0,
                subscribed INTEGER NOT NULL DEFAULT 1,
                sort_order INTEGER NOT NULL DEFAULT 0,
                UNIQUE(account_id, href)
            );

            CREATE TABLE IF NOT EXISTS objects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                calendar_id INTEGER NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
                href TEXT NOT NULL,
                etag TEXT,
                uid TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                location TEXT NOT NULL DEFAULT '',
                dtstart TEXT,
                dtend TEXT,
                all_day INTEGER NOT NULL DEFAULT 0,
                rrule TEXT,
                raw_ics TEXT NOT NULL,
                status TEXT,
                organizer TEXT,
                attendees_json TEXT NOT NULL DEFAULT '[]',
                alarms_json TEXT NOT NULL DEFAULT '[]',
                my_partstat TEXT,
                updated_at TEXT NOT NULL,
                UNIQUE(calendar_id, href)
            );

            CREATE INDEX IF NOT EXISTS idx_objects_uid ON objects(uid);
            CREATE INDEX IF NOT EXISTS idx_objects_dtstart ON objects(dtstart);
            CREATE INDEX IF NOT EXISTS idx_objects_partstat ON objects(my_partstat);

            CREATE TABLE IF NOT EXISTS outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                calendar_id INTEGER NOT NULL,
                href TEXT,
                etag TEXT,
                uid TEXT NOT NULL,
                raw_ics TEXT,
                op TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS alarm_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                uid TEXT NOT NULL,
                trigger_at TEXT NOT NULL,
                fired_at TEXT NOT NULL,
                UNIQUE(uid, trigger_at)
            );

            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        // Additive migrations for existing installs
        let _ = conn.execute(
            "ALTER TABLE calendars ADD COLUMN subscribed INTEGER NOT NULL DEFAULT 1",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE calendars ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(())
    }

    fn map_calendar_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarRow> {
        Ok(CalendarRow {
            id: r.get(0)?,
            account_id: r.get(1)?,
            href: r.get(2)?,
            displayname: r.get(3)?,
            color: r.get(4)?,
            sync_token: r.get(5)?,
            ctag: r.get(6)?,
            visible: r.get::<_, i32>(7)? != 0,
            readonly: r.get::<_, i32>(8)? != 0,
            subscribed: r.get::<_, i32>(9)? != 0,
            sort_order: r.get(10)?,
        })
    }

    pub fn upsert_calendar(
        &self,
        account_id: &str,
        href: &str,
        displayname: &str,
        color: &str,
        readonly: bool,
    ) -> anyhow::Result<i64> {
        let conn = self.conn.lock().unwrap();
        let next_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM calendars WHERE account_id = ?1",
                params![account_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            r#"
            INSERT INTO calendars (account_id, href, displayname, color, readonly, sort_order)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(account_id, href) DO UPDATE SET
                displayname = excluded.displayname,
                color = CASE WHEN calendars.color = '#829dd4' OR calendars.color = excluded.color
                             THEN excluded.color ELSE calendars.color END,
                readonly = excluded.readonly
            "#,
            params![account_id, href, displayname, color, readonly as i32, next_order],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM calendars WHERE account_id = ?1 AND href = ?2",
            params![account_id, href],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn set_calendar_sync_state(
        &self,
        id: i64,
        sync_token: Option<&str>,
        ctag: Option<&str>,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE calendars SET sync_token = ?1, ctag = ?2 WHERE id = ?3",
            params![sync_token, ctag, id],
        )?;
        Ok(())
    }

    pub fn set_calendar_visible(&self, id: i64, visible: bool) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE calendars SET visible = ?1 WHERE id = ?2",
            params![visible as i32, id],
        )?;
        Ok(())
    }

    pub fn set_calendar_color(&self, id: i64, color: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE calendars SET color = ?1 WHERE id = ?2",
            params![color, id],
        )?;
        Ok(())
    }

    pub fn set_calendar_subscribed(&self, id: i64, subscribed: bool) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        if !subscribed {
            conn.execute(
                "DELETE FROM objects WHERE calendar_id = ?1",
                params![id],
            )?;
            // Clear sync token so a later resubscribe does a full fetch
            conn.execute(
                "UPDATE calendars SET subscribed = 0, sync_token = NULL, visible = 0 WHERE id = ?1",
                params![id],
            )?;
            if let Ok(Some(def)) = Self::get_meta_unlocked(&conn, META_DEFAULT_CALENDAR_ID) {
                if def == id.to_string() {
                    conn.execute(
                        "DELETE FROM meta WHERE key = ?1",
                        params![META_DEFAULT_CALENDAR_ID],
                    )?;
                }
            }
        } else {
            conn.execute(
                "UPDATE calendars SET subscribed = 1, visible = 1 WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    pub fn set_default_calendar(&self, id: Option<i64>) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        match id {
            Some(id) => {
                let exists: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM calendars WHERE id = ?1 AND subscribed = 1 AND readonly = 0",
                        params![id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if exists.is_none() {
                    anyhow::bail!("calendar not found or not writable");
                }
                conn.execute(
                    "INSERT INTO meta(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![META_DEFAULT_CALENDAR_ID, id.to_string()],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM meta WHERE key = ?1",
                    params![META_DEFAULT_CALENDAR_ID],
                )?;
            }
        }
        Ok(())
    }

    pub fn get_default_calendar_id(&self) -> anyhow::Result<Option<i64>> {
        let raw = self.get_meta(META_DEFAULT_CALENDAR_ID)?;
        Ok(raw.and_then(|s| s.parse().ok()))
    }

    pub fn reorder_calendars(&self, account_id: &str, ordered_ids: &[i64]) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for (idx, id) in ordered_ids.iter().enumerate() {
            let n = tx.execute(
                "UPDATE calendars SET sort_order = ?1 WHERE id = ?2 AND account_id = ?3",
                params![idx as i64, id, account_id],
            )?;
            if n == 0 {
                anyhow::bail!("calendar {id} not found for account");
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn get_meta_unlocked(conn: &Connection, key: &str) -> anyhow::Result<Option<String>> {
        let v = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn list_calendars(&self) -> anyhow::Result<Vec<CalendarRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, account_id, href, displayname, color, sync_token, ctag, visible, readonly,
                   subscribed, sort_order
            FROM calendars ORDER BY account_id, sort_order, displayname
            "#,
        )?;
        let rows = stmt
            .query_map([], Self::map_calendar_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_calendar(&self, id: i64) -> anyhow::Result<Option<CalendarRow>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                r#"
                SELECT id, account_id, href, displayname, color, sync_token, ctag, visible, readonly,
                       subscribed, sort_order
                FROM calendars WHERE id = ?1
                "#,
                params![id],
                Self::map_calendar_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn upsert_object(
        &self,
        calendar_id: i64,
        href: &str,
        etag: Option<&str>,
        uid: &str,
        summary: &str,
        description: &str,
        location: &str,
        dtstart: Option<&str>,
        dtend: Option<&str>,
        all_day: bool,
        rrule: Option<&str>,
        raw_ics: &str,
        status: Option<&str>,
        organizer: Option<&str>,
        attendees_json: &str,
        alarms_json: &str,
        my_partstat: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO objects (
                calendar_id, href, etag, uid, summary, description, location,
                dtstart, dtend, all_day, rrule, raw_ics, status, organizer,
                attendees_json, alarms_json, my_partstat, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
            )
            ON CONFLICT(calendar_id, href) DO UPDATE SET
                etag = excluded.etag,
                uid = excluded.uid,
                summary = excluded.summary,
                description = excluded.description,
                location = excluded.location,
                dtstart = excluded.dtstart,
                dtend = excluded.dtend,
                all_day = excluded.all_day,
                rrule = excluded.rrule,
                raw_ics = excluded.raw_ics,
                status = excluded.status,
                organizer = excluded.organizer,
                attendees_json = excluded.attendees_json,
                alarms_json = excluded.alarms_json,
                my_partstat = excluded.my_partstat,
                updated_at = excluded.updated_at
            "#,
            params![
                calendar_id,
                href,
                etag,
                uid,
                summary,
                description,
                location,
                dtstart,
                dtend,
                all_day as i32,
                rrule,
                raw_ics,
                status,
                organizer,
                attendees_json,
                alarms_json,
                my_partstat,
                now
            ],
        )?;
        Ok(())
    }

    pub fn delete_object_by_href(&self, calendar_id: i64, href: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM objects WHERE calendar_id = ?1 AND href = ?2",
            params![calendar_id, href],
        )?;
        Ok(())
    }

    pub fn delete_object_by_id(&self, id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM objects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_object(&self, id: i64) -> anyhow::Result<Option<EventRow>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                r#"
                SELECT id, calendar_id, href, etag, uid, summary, description, location,
                       dtstart, dtend, all_day, rrule, raw_ics, status, organizer,
                       attendees_json, alarms_json, my_partstat
                FROM objects WHERE id = ?1
                "#,
                params![id],
                map_event_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn get_object_by_uid(&self, uid: &str) -> anyhow::Result<Option<EventRow>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                r#"
                SELECT id, calendar_id, href, etag, uid, summary, description, location,
                       dtstart, dtend, all_day, rrule, raw_ics, status, organizer,
                       attendees_json, alarms_json, my_partstat
                FROM objects WHERE uid = ?1 LIMIT 1
                "#,
                params![uid],
                map_event_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn get_object_by_href(
        &self,
        calendar_id: i64,
        href: &str,
    ) -> anyhow::Result<Option<EventRow>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                r#"
                SELECT id, calendar_id, href, etag, uid, summary, description, location,
                       dtstart, dtend, all_day, rrule, raw_ics, status, organizer,
                       attendees_json, alarms_json, my_partstat
                FROM objects WHERE calendar_id = ?1 AND href = ?2
                "#,
                params![calendar_id, href],
                map_event_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_events(&self, only_visible: bool) -> anyhow::Result<Vec<EventRow>> {
        let conn = self.conn.lock().unwrap();
        let sql = if only_visible {
            r#"
            SELECT o.id, o.calendar_id, o.href, o.etag, o.uid, o.summary, o.description, o.location,
                   o.dtstart, o.dtend, o.all_day, o.rrule, o.raw_ics, o.status, o.organizer,
                   o.attendees_json, o.alarms_json, o.my_partstat
            FROM objects o
            JOIN calendars c ON c.id = o.calendar_id
            WHERE c.visible = 1 AND c.subscribed = 1
            ORDER BY o.dtstart
            "#
        } else {
            r#"
            SELECT id, calendar_id, href, etag, uid, summary, description, location,
                   dtstart, dtend, all_day, rrule, raw_ics, status, organizer,
                   attendees_json, alarms_json, my_partstat
            FROM objects
            ORDER BY dtstart
            "#
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt
            .query_map([], map_event_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn search_events(&self, query: &str) -> anyhow::Result<Vec<EventRow>> {
        let q = format!("%{}%", query.to_lowercase());
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT o.id, o.calendar_id, o.href, o.etag, o.uid, o.summary, o.description, o.location,
                   o.dtstart, o.dtend, o.all_day, o.rrule, o.raw_ics, o.status, o.organizer,
                   o.attendees_json, o.alarms_json, o.my_partstat
            FROM objects o
            JOIN calendars c ON c.id = o.calendar_id
            WHERE c.visible = 1 AND c.subscribed = 1 AND (
                lower(o.summary) LIKE ?1 OR
                lower(o.description) LIKE ?1 OR
                lower(o.location) LIKE ?1
            )
            ORDER BY o.dtstart
            LIMIT 200
            "#,
        )?;
        let rows = stmt
            .query_map(params![q], map_event_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn pending_invites(&self) -> anyhow::Result<Vec<EventRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT o.id, o.calendar_id, o.href, o.etag, o.uid, o.summary, o.description, o.location,
                   o.dtstart, o.dtend, o.all_day, o.rrule, o.raw_ics, o.status, o.organizer,
                   o.attendees_json, o.alarms_json, o.my_partstat
            FROM objects o
            JOIN calendars c ON c.id = o.calendar_id
            WHERE c.subscribed = 1 AND o.my_partstat = 'NEEDS-ACTION'
            ORDER BY o.dtstart
            "#,
        )?;
        let rows = stmt
            .query_map([], map_event_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn next_event_after(&self, after: &DateTime<Utc>) -> anyhow::Result<Option<EventRow>> {
        let after_s = after.to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                r#"
                SELECT o.id, o.calendar_id, o.href, o.etag, o.uid, o.summary, o.description, o.location,
                       o.dtstart, o.dtend, o.all_day, o.rrule, o.raw_ics, o.status, o.organizer,
                       o.attendees_json, o.alarms_json, o.my_partstat
                FROM objects o
                JOIN calendars c ON c.id = o.calendar_id
                WHERE c.visible = 1 AND c.subscribed = 1 AND o.dtstart IS NOT NULL AND o.dtstart >= ?1
                ORDER BY o.dtstart ASC
                LIMIT 1
                "#,
                params![after_s],
                map_event_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let v = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn mark_alarm_fired(&self, uid: &str, trigger_at: &str) -> anyhow::Result<bool> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "INSERT OR IGNORE INTO alarm_log(uid, trigger_at, fired_at) VALUES(?1, ?2, ?3)",
            params![uid, trigger_at, now],
        )?;
        Ok(changed > 0)
    }

    pub fn remove_calendars_for_account(&self, account_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM calendars WHERE account_id = ?1",
            params![account_id],
        )?;
        Ok(())
    }

    /// Re-parse raw ICS into indexed columns (fixes VTIMEZONE RRULE mis-parse, PARTSTAT dupes).
    pub fn repair_derived_ics_fields(
        &self,
        addresses_by_account: &std::collections::HashMap<String, Vec<String>>,
    ) -> anyhow::Result<usize> {
        self.repair_derived_ics_fields_with_tz(addresses_by_account, None)
    }

    pub fn repair_derived_ics_fields_with_tz(
        &self,
        addresses_by_account: &std::collections::HashMap<String, Vec<String>>,
        default_tz: Option<chrono_tz::Tz>,
    ) -> anyhow::Result<usize> {
        let rows: Vec<(i64, i64, String, String)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                r#"
                SELECT o.id, o.calendar_id, o.raw_ics, c.account_id
                FROM objects o
                JOIN calendars c ON c.id = o.calendar_id
                "#,
            )?;
            let mapped = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            mapped
        };
        let mut fixed = 0usize;
        for (id, _cal_id, raw, account_id) in rows {
            let addrs = addresses_by_account
                .get(&account_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let parsed_opt = if let Some(tz) = default_tz {
                crate::ics::parse_ics_with_tz(&raw, addrs, Some(tz))
            } else {
                crate::ics::parse_ics(&raw, addrs)
            };
            let Some(parsed) = parsed_opt else {
                continue;
            };
            let attendees_json =
                serde_json::to_string(&parsed.attendees).unwrap_or_else(|_| "[]".into());
            let conn = self.conn.lock().unwrap();
            // Also repair dtstart/dtend that were mis-parsed as UTC for floating/TZID
            let changed = conn.execute(
                r#"
                UPDATE objects SET
                    dtstart = ?1,
                    dtend = ?2,
                    all_day = ?3,
                    rrule = ?4,
                    my_partstat = ?5,
                    attendees_json = ?6,
                    organizer = ?7,
                    status = ?8
                WHERE id = ?9 AND (
                    IFNULL(dtstart, '') != IFNULL(?1, '') OR
                    IFNULL(dtend, '') != IFNULL(?2, '') OR
                    all_day != ?3 OR
                    IFNULL(rrule, '') != IFNULL(?4, '') OR
                    IFNULL(my_partstat, '') != IFNULL(?5, '') OR
                    attendees_json != ?6 OR
                    IFNULL(organizer, '') != IFNULL(?7, '') OR
                    IFNULL(status, '') != IFNULL(?8, '')
                )
                "#,
                params![
                    parsed.dtstart,
                    parsed.dtend,
                    parsed.all_day as i32,
                    parsed.rrule,
                    parsed.my_partstat,
                    attendees_json,
                    parsed.organizer,
                    parsed.status,
                    id
                ],
            )?;
            if changed > 0 {
                fixed += 1;
            }
        }
        Ok(fixed)
    }

    /// Own (non-shared) writable calendar copy of an event, if any.
    pub fn find_own_writable_object_by_uid(
        &self,
        uid: &str,
        account_id: &str,
    ) -> anyhow::Result<Option<EventRow>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                r#"
                SELECT o.id, o.calendar_id, o.href, o.etag, o.uid, o.summary, o.description, o.location,
                       o.dtstart, o.dtend, o.all_day, o.rrule, o.raw_ics, o.status, o.organizer,
                       o.attendees_json, o.alarms_json, o.my_partstat
                FROM objects o
                JOIN calendars c ON c.id = o.calendar_id
                WHERE o.uid = ?1 AND c.account_id = ?2 AND c.subscribed = 1
                  AND c.readonly = 0 AND c.href NOT LIKE '%shared_by%'
                ORDER BY o.id LIMIT 1
                "#,
                params![uid, account_id],
                map_event_row,
            )
            .optional()?;
        Ok(row)
    }
}

fn map_event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: r.get(0)?,
        calendar_id: r.get(1)?,
        href: r.get(2)?,
        etag: r.get(3)?,
        uid: r.get(4)?,
        summary: r.get(5)?,
        description: r.get(6)?,
        location: r.get(7)?,
        dtstart: r.get(8)?,
        dtend: r.get(9)?,
        all_day: r.get::<_, i32>(10)? != 0,
        rrule: r.get(11)?,
        raw_ics: r.get(12)?,
        status: r.get(13)?,
        organizer: r.get(14)?,
        attendees_json: r.get(15)?,
        alarms_json: r.get(16)?,
        my_partstat: r.get(17)?,
    })
}
