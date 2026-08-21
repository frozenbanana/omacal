use crate::db::{AlarmInfo, AttendeeInfo};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use icalendar::{
    Calendar, CalendarComponent, Component, DatePerhapsTime, Event, EventLike, EventStatus,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedEvent {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub dtstart: Option<String>, // RFC3339 or YYYY-MM-DD for all-day
    pub dtend: Option<String>,
    pub all_day: bool,
    pub rrule: Option<String>,
    pub status: Option<String>,
    pub organizer: Option<String>,
    pub attendees: Vec<AttendeeInfo>,
    pub alarms: Vec<AlarmInfo>,
    pub my_partstat: Option<String>,
    pub raw_ics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventInput {
    pub calendar_id: i64,
    pub uid: Option<String>,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub dtstart: String,
    pub dtend: String,
    pub all_day: bool,
    pub timezone: String,
    pub rrule: Option<String>,
    pub alarms: Vec<AlarmInfo>,
    pub attendees: Vec<AttendeeInfo>,
    pub href: Option<String>,
    pub etag: Option<String>,
}

pub fn default_tz() -> Tz {
    "Europe/Stockholm".parse().unwrap_or(chrono_tz::UTC)
}

/// Public entry — uses default Stockholm TZ for floating times (legacy).
pub fn parse_ics(raw: &str, my_addresses: &[String]) -> Option<ParsedEvent> {
    parse_ics_with_tz(raw, my_addresses, Some(default_tz()))
}

/// Parse with an explicit default timezone for floating (no TZID, no Z) values.
pub fn parse_ics_with_tz(
    raw: &str,
    my_addresses: &[String],
    default_tz: Option<Tz>,
) -> Option<ParsedEvent> {
    let cal: Calendar = raw.parse().ok()?;
    let event = cal.components.iter().find_map(|c| match c {
        CalendarComponent::Event(e) => Some(e),
        _ => None,
    })?;
    let uid = event
        .get_uid()
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let summary = event.get_summary().unwrap_or("").to_string();
    let description = event.get_description().unwrap_or("").to_string();
    let location = event.get_location().unwrap_or("").to_string();

    let (dtstart, all_day) = match event.get_start() {
        Some(s) => date_perhaps_to_strings_with_tz(s, default_tz),
        None => extract_dt_from_raw_with_tz(raw, "DTSTART", default_tz),
    };
    let (dtend, _) = match event.get_end() {
        Some(s) => date_perhaps_to_strings_with_tz(s, default_tz),
        None => extract_dt_from_raw_with_tz(raw, "DTEND", default_tz),
    };

    // RRULE/ORGANIZER/ATTENDEE must come from VEVENT — VTIMEZONE also has RRULE lines
    let rrule = property_in_vevent(raw, "RRULE");
    let status = event.get_status().map(|s| match s {
        EventStatus::Tentative => "TENTATIVE".into(),
        EventStatus::Cancelled => "CANCELLED".into(),
        EventStatus::Confirmed => "CONFIRMED".into(),
    });

    let organizer = property_mailto_in_vevent(raw, "ORGANIZER");
    let attendees = parse_attendees(raw);
    let alarms = parse_alarms(raw);

    let my_partstat = effective_my_partstat(&attendees, my_addresses);

    Some(ParsedEvent {
        uid,
        summary,
        description,
        location,
        dtstart,
        dtend,
        all_day,
        rrule,
        status,
        organizer,
        attendees,
        alarms,
        my_partstat,
        raw_ics: raw.to_string(),
    })
}

/// Lightweight parse for .ics imports (no attendee matching needed).
pub fn preview_from_ics(raw: &str) -> Option<ParsedEvent> {
    parse_ics(raw, &[])
}

pub fn preview_from_ics_with_tz(raw: &str, default_tz: Option<Tz>) -> Option<ParsedEvent> {
    parse_ics_with_tz(raw, &[], default_tz)
}

fn emails_equal(a: &str, b: &str) -> bool {
    let na = a.trim().trim_start_matches("mailto:").to_lowercase();
    let nb = b.trim().trim_start_matches("mailto:").to_lowercase();
    na == nb
}

#[allow(dead_code)]
fn date_perhaps_to_strings(dt: DatePerhapsTime) -> (Option<String>, bool) {
    date_perhaps_to_strings_with_tz(dt, Some(default_tz()))
}

fn date_perhaps_to_strings_with_tz(
    dt: DatePerhapsTime,
    default_tz: Option<Tz>,
) -> (Option<String>, bool) {
    match dt {
        DatePerhapsTime::Date(d) => (Some(d.format("%Y-%m-%d").to_string()), true),
        DatePerhapsTime::DateTime(cdt) => calendar_date_time_to_strings_with_tz(cdt, default_tz),
    }
}

#[allow(dead_code)]
fn calendar_date_time_to_strings(dt: icalendar::CalendarDateTime) -> (Option<String>, bool) {
    calendar_date_time_to_strings_with_tz(dt, Some(default_tz()))
}

fn calendar_date_time_to_strings_with_tz(
    dt: icalendar::CalendarDateTime,
    default_tz: Option<Tz>,
) -> (Option<String>, bool) {
    use icalendar::CalendarDateTime as CDT;
    match dt {
        CDT::Floating(ndt) => {
            if let Some(tz) = default_tz {
                if let Some(ldt) = tz.from_local_datetime(&ndt).single() {
                    return (Some(ldt.with_timezone(&Utc).to_rfc3339()), false);
                }
                if let Some(ldt) = tz.from_local_datetime(&ndt).earliest() {
                    return (Some(ldt.with_timezone(&Utc).to_rfc3339()), false);
                }
            }
            (
                Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc).to_rfc3339()),
                false,
            )
        }
        CDT::Utc(dt) => (Some(dt.to_rfc3339()), false),
        CDT::WithTimezone { date_time, tzid } => {
            if let Ok(tz) = tzid.parse::<Tz>() {
                if let Some(ldt) = tz.from_local_datetime(&date_time).single() {
                    return (Some(ldt.with_timezone(&Utc).to_rfc3339()), false);
                }
                if let Some(ldt) = tz.from_local_datetime(&date_time).earliest() {
                    return (Some(ldt.with_timezone(&Utc).to_rfc3339()), false);
                }
            }
            (
                Some(DateTime::<Utc>::from_naive_utc_and_offset(date_time, Utc).to_rfc3339()),
                false,
            )
        }
    }
}

#[allow(dead_code)]
fn extract_dt_from_raw(raw: &str, name: &str) -> (Option<String>, bool) {
    extract_dt_from_raw_with_tz(raw, name, Some(default_tz()))
}

fn extract_dt_from_raw_with_tz(
    raw: &str,
    name: &str,
    default_tz: Option<Tz>,
) -> (Option<String>, bool) {
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if !upper.starts_with(name) {
            continue;
        }
        let all_day = upper.contains("VALUE=DATE");
        if let Some(idx) = line.find(':') {
            let val = line[idx + 1..].trim();
            if all_day || val.len() == 8 {
                if let Ok(d) = NaiveDate::parse_from_str(val, "%Y%m%d") {
                    return (Some(d.format("%Y-%m-%d").to_string()), true);
                }
            }
            if let Ok(dt) = DateTime::parse_from_str(val, "%Y%m%dT%H%M%SZ") {
                return (Some(dt.with_timezone(&Utc).to_rfc3339()), false);
            }
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(val, "%Y%m%dT%H%M%S") {
                if let Some(tz) = default_tz {
                    if let Some(ldt) = tz.from_local_datetime(&ndt).single() {
                        return (Some(ldt.with_timezone(&Utc).to_rfc3339()), false);
                    }
                    if let Some(ldt) = tz.from_local_datetime(&ndt).earliest() {
                        return (Some(ldt.with_timezone(&Utc).to_rfc3339()), false);
                    }
                }
                return (
                    Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc).to_rfc3339()),
                    false,
                );
            }
        }
    }
    (None, false)
}

/// Extract wall (Naive) + TZID for a DTSTART/DTEND line from raw ICS.
/// Returns (wall_naive, tz_option, is_all_day)
pub fn extract_wall_dt_and_tz(
    raw: &str,
    prop: &str,
) -> Option<(chrono::NaiveDateTime, Option<Tz>, bool)> {
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if !upper.starts_with(prop) {
            continue;
        }
        if upper.contains("VALUE=DATE") {
            continue;
        }
        if let Some(idx) = line.find(':') {
            let left = &line[..idx];
            let val = line[idx + 1..].trim();
            // try to get TZID from params
            let tz_opt = if let Some(tzid_idx) = left.to_uppercase().find("TZID=") {
                let after = &left[tzid_idx + 5..];
                let end = after.find(';').unwrap_or(after.len());
                let tzid_raw = after[..end].trim_matches('"');
                tzid_raw.parse::<Tz>().ok()
            } else {
                None
            };
            if val.ends_with('Z') {
                if let Ok(dt) = DateTime::parse_from_str(val, "%Y%m%dT%H%M%SZ") {
                    let utc = dt.with_timezone(&Utc);
                    // convert to wall in that tz if available
                    if let Some(tz) = tz_opt {
                        return Some((utc.with_timezone(&tz).naive_local(), Some(tz), false));
                    } else {
                        // Z is UTC wall -> keep naive UTC
                        return Some((utc.naive_utc(), None, false));
                    }
                }
            }
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(val, "%Y%m%dT%H%M%S") {
                return Some((ndt, tz_opt, false));
            }
        }
    }
    None
}

fn property_in_vevent(raw: &str, name: &str) -> Option<String> {
    property_in_component(raw, name, Some("VEVENT"))
}

fn property_in_component(raw: &str, name: &str, component: Option<&str>) -> Option<String> {
    let mut in_component = component.is_none();
    let begin = component.map(|c| format!("BEGIN:{c}"));
    let end = component.map(|c| format!("END:{c}"));
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if let Some(b) = &begin {
            if upper == *b {
                in_component = true;
                continue;
            }
        }
        if let Some(e) = &end {
            if upper == *e {
                in_component = false;
                continue;
            }
        }
        if !in_component {
            continue;
        }
        if upper.starts_with(name)
            && (upper.len() == name.len()
                || upper.as_bytes().get(name.len()) == Some(&b';')
                || upper.as_bytes().get(name.len()) == Some(&b':'))
        {
            if let Some(idx) = line.find(':') {
                return Some(line[idx + 1..].trim().to_string());
            }
        }
    }
    None
}

fn property_mailto_in_vevent(raw: &str, name: &str) -> Option<String> {
    property_in_vevent(raw, name).map(|v| {
        v.trim()
            .trim_start_matches("mailto:")
            .trim_start_matches("MAILTO:")
            .to_string()
    })
}

/// When ICS has duplicate ATTENDEE lines for the same address, prefer a decided
/// PARTSTAT over NEEDS-ACTION / missing.
pub fn effective_my_partstat(
    attendees: &[crate::db::AttendeeInfo],
    my_addresses: &[String],
) -> Option<String> {
    let mut best: Option<String> = None;
    let mut best_rank = u8::MAX;
    for a in attendees {
        if !my_addresses.iter().any(|m| emails_equal(m, &a.email)) {
            continue;
        }
        let Some(p) = a.partstat.as_deref() else {
            continue;
        };
        let rank = partstat_rank(p);
        if rank < best_rank {
            best_rank = rank;
            best = Some(p.to_uppercase());
        }
    }
    best
}

fn partstat_rank(p: &str) -> u8 {
    match p.to_uppercase().as_str() {
        "ACCEPTED" => 0,
        "DECLINED" => 1,
        "TENTATIVE" => 2,
        "NEEDS-ACTION" => 3,
        other if !other.is_empty() => 4,
        _ => 5,
    }
}

fn unfold(raw: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for line in raw.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            current.push_str(line.trim_start());
        } else {
            if !current.is_empty() {
                lines.push(current);
            }
            current = line.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn parse_attendees(raw: &str) -> Vec<AttendeeInfo> {
    let mut out = Vec::new();
    let mut in_vevent = false;
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_vevent = true;
            continue;
        }
        if upper == "END:VEVENT" {
            in_vevent = false;
            continue;
        }
        if !in_vevent || !upper.starts_with("ATTENDEE") {
            continue;
        }
        let (params, value) = split_prop(&line);
        let email = value
            .trim()
            .trim_start_matches("mailto:")
            .trim_start_matches("MAILTO:")
            .to_string();
        out.push(AttendeeInfo {
            email,
            cn: params.get("CN").cloned(),
            partstat: params.get("PARTSTAT").cloned(),
            role: params.get("ROLE").cloned(),
            rsvp: params
                .get("RSVP")
                .map(|v| v.eq_ignore_ascii_case("TRUE"))
                .unwrap_or(false),
        });
    }
    out
}

fn parse_alarms(raw: &str) -> Vec<AlarmInfo> {
    let mut out = Vec::new();
    let mut in_alarm = false;
    let mut trigger = None;
    let mut description = None;
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if upper == "BEGIN:VALARM" {
            in_alarm = true;
            trigger = None;
            description = None;
            continue;
        }
        if upper == "END:VALARM" {
            if let Some(t) = trigger.take() {
                out.push(AlarmInfo {
                    trigger: t,
                    description: description.take(),
                });
            }
            in_alarm = false;
            continue;
        }
        if !in_alarm {
            continue;
        }
        if upper.starts_with("TRIGGER") {
            if let Some(idx) = line.find(':') {
                trigger = Some(line[idx + 1..].trim().to_string());
            }
        }
        if upper.starts_with("DESCRIPTION") {
            if let Some(idx) = line.find(':') {
                description = Some(line[idx + 1..].trim().to_string());
            }
        }
    }
    out
}

fn split_prop(line: &str) -> (std::collections::HashMap<String, String>, String) {
    let mut params = std::collections::HashMap::new();
    let (left, value) = match line.split_once(':') {
        Some((l, v)) => (l, v.to_string()),
        None => return (params, line.to_string()),
    };
    let mut parts = left.split(';');
    let _name = parts.next();
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            params.insert(k.to_uppercase(), v.trim_matches('"').to_string());
        }
    }
    (params, value)
}

pub fn build_ics(
    input: &EventInput,
    existing_uid: Option<&str>,
) -> Result<(String, String), String> {
    let uid = existing_uid
        .map(|s| s.to_string())
        .or_else(|| input.uid.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut event = Event::new();
    event.uid(&uid);
    event.summary(&input.summary);
    if !input.description.is_empty() {
        event.description(&input.description);
    }
    if !input.location.is_empty() {
        event.location(&input.location);
    }
    // done() mem::takes inner properties — must push the returned event
    let event = event.done();

    // icalendar builder API is limited for TZID / attendees / alarms — append manually
    let mut cal = Calendar::new();
    cal.push(event);

    let mut ics = cal.to_string();
    // Replace DTSTART/DTEND if builder didn't set them properly
    ics = inject_times(&ics, input)?;
    ics = inject_rrule(&ics, input.rrule.as_deref());
    ics = inject_attendees(&ics, &input.attendees);
    ics = inject_alarms(&ics, &input.alarms);

    Ok((uid, ics))
}

fn inject_times(ics: &str, input: &EventInput) -> Result<String, String> {
    let (start_line, end_line) = if input.all_day {
        let s =
            NaiveDate::parse_from_str(&input.dtstart[..10.min(input.dtstart.len())], "%Y-%m-%d")
                .or_else(|_| NaiveDate::parse_from_str(&input.dtstart, "%Y-%m-%d"))
                .map_err(|e| e.to_string())?;
        let e = NaiveDate::parse_from_str(&input.dtend[..10.min(input.dtend.len())], "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(&input.dtend, "%Y-%m-%d"))
            .map_err(|e| e.to_string())?;
        (
            format!("DTSTART;VALUE=DATE:{}", s.format("%Y%m%d")),
            format!("DTEND;VALUE=DATE:{}", e.format("%Y%m%d")),
        )
    } else {
        let tz: Tz = input
            .timezone
            .parse()
            .map_err(|_| format!("invalid timezone {}", input.timezone))?;
        let start = parse_local_to_tz(&input.dtstart, tz)?;
        let end = parse_local_to_tz(&input.dtend, tz)?;
        (
            format!(
                "DTSTART;TZID={}:{}",
                input.timezone,
                start.format("%Y%m%dT%H%M%S")
            ),
            format!(
                "DTEND;TZID={}:{}",
                input.timezone,
                end.format("%Y%m%dT%H%M%S")
            ),
        )
    };

    let mut out = String::new();
    let mut inserted = false;
    for line in ics.lines() {
        let u = line.to_uppercase();
        if u.starts_with("DTSTART") || u.starts_with("DTEND") {
            continue;
        }
        if u.starts_with("UID:") && !inserted {
            out.push_str(line);
            out.push_str("\r\n");
            out.push_str(&start_line);
            out.push_str("\r\n");
            out.push_str(&end_line);
            out.push_str("\r\n");
            inserted = true;
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    if !inserted {
        // fallback insert before END:VEVENT
        out = ics.replace(
            "END:VEVENT",
            &format!("{start_line}\r\n{end_line}\r\nEND:VEVENT"),
        );
    }
    Ok(out)
}

fn parse_local_to_tz(s: &str, tz: Tz) -> Result<chrono::NaiveDateTime, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&tz).naive_local());
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(ndt);
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(ndt);
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Ok(ndt);
    }
    Err(format!("cannot parse datetime: {s}"))
}

fn inject_rrule(ics: &str, rrule: Option<&str>) -> String {
    match rrule {
        Some(r) if !r.is_empty() => {
            let line = if r.to_uppercase().starts_with("RRULE:") {
                r.to_string()
            } else {
                format!("RRULE:{r}")
            };
            ics.replace("END:VEVENT", &format!("{line}\r\nEND:VEVENT"))
        }
        _ => ics.to_string(),
    }
}

fn inject_attendees(ics: &str, attendees: &[AttendeeInfo]) -> String {
    if attendees.is_empty() {
        return ics.to_string();
    }
    let mut block = String::new();
    for a in attendees {
        let mut line = String::from("ATTENDEE");
        if let Some(cn) = &a.cn {
            line.push_str(&format!(";CN={}", escape_param(cn)));
        }
        if let Some(ps) = &a.partstat {
            line.push_str(&format!(";PARTSTAT={ps}"));
        } else {
            line.push_str(";PARTSTAT=NEEDS-ACTION");
        }
        if let Some(role) = &a.role {
            line.push_str(&format!(";ROLE={role}"));
        } else {
            line.push_str(";ROLE=REQ-PARTICIPANT");
        }
        if a.rsvp {
            line.push_str(";RSVP=TRUE");
        }
        line.push_str(&format!(":mailto:{}", a.email));
        block.push_str(&line);
        block.push_str("\r\n");
    }
    ics.replace("END:VEVENT", &format!("{block}END:VEVENT"))
}

fn inject_alarms(ics: &str, alarms: &[AlarmInfo]) -> String {
    if alarms.is_empty() {
        return ics.to_string();
    }
    let mut block = String::new();
    for a in alarms {
        block.push_str("BEGIN:VALARM\r\n");
        block.push_str("ACTION:DISPLAY\r\n");
        block.push_str(&format!("TRIGGER:{}\r\n", a.trigger));
        if let Some(d) = &a.description {
            block.push_str(&format!("DESCRIPTION:{d}\r\n"));
        } else {
            block.push_str("DESCRIPTION:Reminder\r\n");
        }
        block.push_str("END:VALARM\r\n");
    }
    ics.replace("END:VEVENT", &format!("{block}END:VEVENT"))
}

fn escape_param(s: &str) -> String {
    if s.contains([';', ':', ',']) {
        format!("\"{}\"", s.replace('"', "'"))
    } else {
        s.to_string()
    }
}

pub fn set_partstat_in_ics(raw: &str, my_addresses: &[String], partstat: &str) -> String {
    let mut out_lines = Vec::new();
    let mut in_vevent = false;
    let mut rewritten: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if upper == "BEGIN:VEVENT" {
            in_vevent = true;
            rewritten.clear();
            out_lines.push(line);
            continue;
        }
        if upper == "END:VEVENT" {
            in_vevent = false;
            out_lines.push(line);
            continue;
        }
        if in_vevent && upper.starts_with("ATTENDEE") {
            let (_params, value) = split_prop(&line);
            let email = value
                .trim()
                .trim_start_matches("mailto:")
                .trim_start_matches("MAILTO:")
                .to_string();
            let email_key = email.to_lowercase();
            if my_addresses.iter().any(|m| emails_equal(m, &email)) {
                // Drop duplicate ATTENDEE rows for the same address
                if !rewritten.insert(email_key) {
                    continue;
                }
                let (mut params, _) = split_prop(&line);
                params.insert("PARTSTAT".into(), partstat.to_string());
                let mut rebuilt = String::from("ATTENDEE");
                for (k, v) in &params {
                    rebuilt.push(';');
                    rebuilt.push_str(k);
                    rebuilt.push('=');
                    rebuilt.push_str(v);
                }
                rebuilt.push_str(&format!(":mailto:{email}"));
                out_lines.push(rebuilt);
                continue;
            }
        }
        out_lines.push(line);
    }
    out_lines.join("\r\n") + "\r\n"
}

/// DST-safe expansion using wall time + TZID.
/// If tz is Some, DTSTART is emitted with TZID so occurrences keep wall time
/// across DST transitions (e.g. 06:00 Stockholm stays 06:00, UTC flips 04:00/05:00).
pub fn expand_rrule_wall_tz(
    wall_start: chrono::NaiveDateTime,
    tz: Option<Tz>,
    rrule: &str,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    use rrule::RRuleSet;
    let dtstart_line = if let Some(tz) = tz {
        format!(
            "DTSTART;TZID={}:{}",
            tz.name(),
            wall_start.format("%Y%m%dT%H%M%S")
        )
    } else {
        format!("DTSTART:{}", wall_start.format("%Y%m%dT%H%M%S"))
    };
    let rule_str = if rrule.to_uppercase().starts_with("RRULE:") {
        format!("{dtstart_line}\n{rrule}")
    } else {
        format!("{dtstart_line}\nRRULE:{rrule}")
    };
    let Ok(set) = rule_str.parse::<RRuleSet>() else {
        // fallback: convert single wall to UTC
        if let Some(tz) = tz {
            if let Some(ldt) = tz
                .from_local_datetime(&wall_start)
                .single()
                .or_else(|| tz.from_local_datetime(&wall_start).earliest())
            {
                return vec![ldt.with_timezone(&Utc)];
            }
        }
        return vec![DateTime::<Utc>::from_naive_utc_and_offset(wall_start, Utc)];
    };
    set.into_iter()
        .skip_while(|d| d.with_timezone(&Utc) < range_start)
        .take_while(|d| d.with_timezone(&Utc) <= range_end)
        .take(500)
        .map(|d| d.with_timezone(&Utc))
        .collect()
}

/// Legacy UTC-based expansion — kept for tests / non-recurring fallback.
/// For recurring wall-time events, prefer `expand_rrule_wall_tz`.
pub fn expand_rrule_occurrences(
    dtstart_rfc: &str,
    rrule: &str,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    use rrule::RRuleSet;
    let start = DateTime::parse_from_rfc3339(dtstart_rfc)
        .map(|d| d.with_timezone(&Utc))
        .ok();
    let Some(start) = start else {
        return vec![];
    };
    let rule_str = if rrule.to_uppercase().starts_with("RRULE:") {
        format!("DTSTART:{}\n{}", start.format("%Y%m%dT%H%M%SZ"), rrule)
    } else {
        format!(
            "DTSTART:{}\nRRULE:{}",
            start.format("%Y%m%dT%H%M%SZ"),
            rrule
        )
    };
    let Ok(set) = rule_str.parse::<RRuleSet>() else {
        return vec![start];
    };
    set.into_iter()
        .skip_while(|d| d.with_timezone(&Utc) < range_start)
        .take_while(|d| d.with_timezone(&Utc) <= range_end)
        .take(500)
        .map(|d| d.with_timezone(&Utc))
        .collect()
}

/// Parse all EXDATE values from raw ICS, returning UTC instants (filtered to same tz logic as DTSTART).
pub fn parse_exdates(raw: &str, default_tz: Option<Tz>) -> Vec<DateTime<Utc>> {
    let mut out = Vec::new();
    for line in unfold(raw) {
        let upper = line.to_uppercase();
        if !upper.starts_with("EXDATE") {
            continue;
        }
        let Some(idx) = line.find(':') else { continue };
        let left = &line[..idx];
        let val_part = line[idx + 1..].trim();
        // Detect TZID for this EXDATE line
        let tz_opt = if let Some(tzid_idx) = left.to_uppercase().find("TZID=") {
            let after = &left[tzid_idx + 5..];
            let end = after.find(';').unwrap_or(after.len());
            let tzid_raw = after[..end].trim_matches('"');
            tzid_raw.parse::<Tz>().ok().or(default_tz)
        } else {
            // Check if values are date-only (VALUE=DATE) — ignore for timed expansion
            if upper.contains("VALUE=DATE") {
                continue;
            }
            None
        };
        for token in val_part.split(',') {
            let tok = token.trim();
            if tok.is_empty() {
                continue;
            }
            let dt_opt = if tok.ends_with('Z') {
                DateTime::parse_from_str(tok, "%Y%m%dT%H%M%SZ")
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            } else if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(tok, "%Y%m%dT%H%M%S") {
                if let Some(tz) = tz_opt {
                    if let Some(ldt) = tz
                        .from_local_datetime(&ndt)
                        .single()
                        .or_else(|| tz.from_local_datetime(&ndt).earliest())
                    {
                        Some(ldt.with_timezone(&Utc))
                    } else {
                        Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                    }
                } else if let Some(tz) = default_tz {
                    // Floating EXDATE -> interpret as default timezone
                    if let Some(ldt) = tz
                        .from_local_datetime(&ndt)
                        .single()
                        .or_else(|| tz.from_local_datetime(&ndt).earliest())
                    {
                        Some(ldt.with_timezone(&Utc))
                    } else {
                        Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                    }
                } else {
                    Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                }
            } else if let Ok(d) = NaiveDate::parse_from_str(tok, "%Y%m%d") {
                // All-day EXDATE — treat as midnight in tz if given
                let ndt = d.and_hms_opt(0, 0, 0).unwrap();
                if let Some(tz) = tz_opt.or(default_tz) {
                    if let Some(ldt) = tz
                        .from_local_datetime(&ndt)
                        .single()
                        .or_else(|| tz.from_local_datetime(&ndt).earliest())
                    {
                        Some(ldt.with_timezone(&Utc))
                    } else {
                        Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                    }
                } else {
                    Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                }
            } else {
                None
            };
            if let Some(dt) = dt_opt {
                out.push(dt);
            }
        }
    }
    out
}

/// Inject a single EXDATE occurrence into raw ICS.
/// Emits EXDATE in same TZID form as DTSTART if possible.
pub fn inject_exdate(raw: &str, occurrence_utc: DateTime<Utc>, dtstart_tz: Option<Tz>) -> String {
    // Dedupe: if already excluded, return unchanged
    let default_tz = dtstart_tz.or(Some(default_tz()));
    let existing = parse_exdates(raw, default_tz);
    if existing
        .iter()
        .any(|d| d.timestamp() == occurrence_utc.timestamp())
    {
        return raw.to_string();
    }
    // Determine EXDATE string form to match DTSTART
    let exdate_val = if let Some(tz) = dtstart_tz {
        let wall = occurrence_utc.with_timezone(&tz).naive_local();
        format!("EXDATE;TZID={}:{}", tz.name(), wall.format("%Y%m%dT%H%M%S"))
    } else {
        // Check if existing EXDATE uses TZID or Z — default to UTC Z
        let mut has_tzid = false;
        for line in unfold(raw) {
            if line.to_uppercase().starts_with("EXDATE") && line.to_uppercase().contains("TZID=") {
                has_tzid = true;
                break;
            }
        }
        if has_tzid {
            // Try to reuse TZID from DTSTART
            if let Some((_, Some(tz), _)) = extract_wall_dt_and_tz(raw, "DTSTART") {
                let wall = occurrence_utc.with_timezone(&tz).naive_local();
                format!("EXDATE;TZID={}:{}", tz.name(), wall.format("%Y%m%dT%H%M%S"))
            } else {
                format!("EXDATE:{}", occurrence_utc.format("%Y%m%dT%H%M%SZ"))
            }
        } else {
            format!("EXDATE:{}", occurrence_utc.format("%Y%m%dT%H%M%SZ"))
        }
    };
    // Append to existing EXDATE line if present, else add before END:VEVENT
    let mut lines = unfold(raw);
    let mut found_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.to_uppercase().starts_with("EXDATE") {
            found_idx = Some(i);
        }
    }
    if let Some(idx) = found_idx {
        let merged = format!(
            "{},{}",
            lines[idx].trim_end(),
            exdate_val.split(':').nth(1).unwrap_or("")
        );
        // If existing line had TZID param, we should keep it — already handled by using same TZID form above
        // Simplify: if we generated TZID form, replace whole line with combined values
        if lines[idx].to_uppercase().contains("TZID=") && exdate_val.contains("TZID=") {
            // Both TZID — merge values after ':'
            let prefix = lines[idx].split(':').next().unwrap_or("EXDATE");
            let existing_vals = lines[idx].split(':').nth(1).unwrap_or("");
            let new_val = exdate_val.split(':').nth(1).unwrap_or("");
            lines[idx] = format!("{}:{},{}", prefix, existing_vals, new_val);
        } else {
            lines[idx] = merged;
        }
    } else {
        // insert before END:VEVENT
        if let Some(pos) = lines.iter().position(|l| l.to_uppercase() == "END:VEVENT") {
            lines.insert(pos, exdate_val);
        } else {
            lines.push(exdate_val);
        }
    }
    lines.join("\r\n") + "\r\n"
}

/// Truncate RRULE with UNTIL (keeps wall semantics). Returns new raw ICS.
pub fn truncate_rrule_until(
    raw: &str,
    until_wall: chrono::NaiveDateTime,
    until_tz: Option<Tz>,
    is_all_day: bool,
) -> String {
    let mut lines = unfold(raw);
    let mut new_lines = Vec::new();
    let until_str = if is_all_day {
        format!("UNTIL={}", until_wall.format("%Y%m%d"))
    } else if let Some(tz) = until_tz {
        // If DTSTART had TZID, UNTIL can be wall as well? Spec says UNTIL must be UTC if DTSTART is UTC, else wall.
        // For our wall TZID DTSTART we emit wall UNTIL without Z (but many servers expect UTC). Emit UTC Z for safety.
        // However to keep wall, we emit UTC Z equivalent.
        let utc = {
            if let Some(ldt) = tz
                .from_local_datetime(&until_wall)
                .single()
                .or_else(|| tz.from_local_datetime(&until_wall).earliest())
            {
                ldt.with_timezone(&Utc)
            } else {
                DateTime::<Utc>::from_naive_utc_and_offset(until_wall, Utc)
            }
        };
        format!("UNTIL={}", utc.format("%Y%m%dT%H%M%SZ"))
    } else {
        DateTime::<Utc>::from_naive_utc_and_offset(until_wall, Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
    };
    let until_str = format!("UNTIL={}", until_str.split('=').nth(1).unwrap_or(""));
    for line in lines.drain(..) {
        let upper = line.to_uppercase();
        if upper.starts_with("RRULE") {
            // parse RRULE, drop COUNT, add/replace UNTIL
            let rrule_val = line.split(':').nth(1).unwrap_or("").to_string();
            // Split into k=v pairs
            let mut parts: Vec<String> = rrule_val.split(';').map(|s| s.to_string()).collect();
            parts.retain(|p| {
                !p.to_uppercase().starts_with("COUNT=") && !p.to_uppercase().starts_with("UNTIL=")
            });
            parts.push(until_str.clone());
            let new_rrule = format!("RRULE:{}", parts.join(";"));
            new_lines.push(new_rrule);
        } else {
            new_lines.push(line);
        }
    }
    new_lines.join("\r\n") + "\r\n"
}

/// Try wall+TZ expansion from raw ICS; fallback to UTC. Also filters EXDATEs.
pub fn expand_rrule_from_raw(
    raw: &str,
    dtstart_rfc: &str,
    rrule: &str,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    let mut occ = if let Some((wall, tz, _)) = extract_wall_dt_and_tz(raw, "DTSTART") {
        // Only use wall path if raw contains TZID or is floating wall time
        // (i.e., we could reconstruct wall). For pure UTC (Z), keep UTC path.
        let is_utc = raw
            .lines()
            .any(|l| l.to_uppercase().contains("DTSTART") && l.contains("Z"));
        if tz.is_some() || !is_utc {
            expand_rrule_wall_tz(wall, tz, rrule, range_start, range_end)
        } else {
            expand_rrule_occurrences(dtstart_rfc, rrule, range_start, range_end)
        }
    } else {
        expand_rrule_occurrences(dtstart_rfc, rrule, range_start, range_end)
    };
    // Filter EXDATEs
    let default_tz = extract_wall_dt_and_tz(raw, "DTSTART")
        .and_then(|(_, tz, _)| tz)
        .or(Some(default_tz()));
    let exdates = parse_exdates(raw, default_tz);
    if !exdates.is_empty() {
        let ex_set: std::collections::HashSet<i64> =
            exdates.iter().map(|d| d.timestamp()).collect();
        occ.retain(|d| !ex_set.contains(&d.timestamp()));
    }
    occ
}

pub fn alarm_trigger_at(event_start: DateTime<Utc>, trigger: &str) -> Option<DateTime<Utc>> {
    // Support -PT15M style relative triggers
    let t = trigger.trim();
    if let Some(rest) = t.strip_prefix('-') {
        let dur = parse_iso_duration(rest)?;
        return Some(event_start - dur);
    }
    if t.starts_with('P') || t.starts_with("PT") {
        let dur = parse_iso_duration(t)?;
        return Some(event_start + dur);
    }
    // Absolute DATE-TIME
    DateTime::parse_from_rfc3339(t)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(t, "%Y%m%dT%H%M%SZ")
                .ok()
                .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc))
        })
}

fn parse_iso_duration(s: &str) -> Option<Duration> {
    // Minimal: PT15M, PT1H, P1D, PT1H30M
    let s = s.trim().trim_start_matches('+');
    if !s.starts_with('P') {
        return None;
    }
    let mut rest = &s[1..];
    let mut total = Duration::zero();
    if let Some(idx) = rest.find('D') {
        let days: i64 = rest[..idx].parse().ok()?;
        total += Duration::days(days);
        rest = &rest[idx + 1..];
    }
    if rest.starts_with('T') {
        rest = &rest[1..];
        let mut num = String::new();
        for ch in rest.chars() {
            if ch.is_ascii_digit() {
                num.push(ch);
            } else {
                let n: i64 = num.parse().ok()?;
                num.clear();
                match ch {
                    'H' => total += Duration::hours(n),
                    'M' => total += Duration::minutes(n),
                    'S' => total += Duration::seconds(n),
                    _ => {}
                }
            }
        }
    }
    Some(total)
}

#[allow(dead_code)]
pub fn default_href_for_uid(calendar_href: &str, uid: &str) -> String {
    let base = calendar_href.trim_end_matches('/');
    format!("{base}/{uid}.ics")
}

#[test]
fn preview_import_parses_event_fields() {
    const ICS: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:import-1\r\n\
DTSTAMP:20260801T120000Z\r\n\
DTSTART:20260914T090000\r\n\
DTEND:20260914T100000\r\n\
SUMMARY:Standup\r\n\
LOCATION:Meeting room\r\n\
DESCRIPTION:Daily sync\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\n\
ATTENDEE;CN=Alice;PARTSTAT=NEEDS-ACTION:mailto:alice@example.com\r\n\
BEGIN:VALARM\r\n\
TRIGGER:-PT10M\r\n\
DESCRIPTION:Reminder\r\n\
ACTION:DISPLAY\r\n\
END:VALARM\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";
    let p = preview_from_ics(ICS).expect("preview");
    assert_eq!(p.summary, "Standup");
    assert_eq!(p.location, "Meeting room");
    assert_eq!(p.description, "Daily sync");
    // Floating 09:00 without TZ is interpreted as Europe/Stockholm.
    // Sep 14 is CEST (UTC+2) => 07:00Z
    assert_eq!(p.dtstart.as_deref(), Some("2026-09-14T07:00:00+00:00"));
    assert!(!p.all_day);
    assert_eq!(
        p.rrule.as_deref(),
        Some("FREQ=WEEKLY;BYDAY=MO,WE"),
        "rrule={:?}",
        p.rrule
    );
    assert_eq!(p.attendees.len(), 1);
    assert_eq!(p.attendees[0].email, "alice@example.com");
    assert_eq!(p.alarms.len(), 1);
    assert_eq!(p.alarms[0].trigger, "-PT10M");
}

#[cfg(test)]

mod tests {
    use super::*;

    const TZ_AND_EVENT: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VTIMEZONE\r\n\
TZID:Europe/Lisbon\r\n\
BEGIN:STANDARD\r\n\
DTSTART:19701025T020000\r\n\
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\n\
END:STANDARD\r\n\
END:VTIMEZONE\r\n\
BEGIN:VEVENT\r\n\
UID:test-onboarding\r\n\
DTSTART:20260810T130000Z\r\n\
DTEND:20260810T140000Z\r\n\
SUMMARY:Onboarding\r\n\
ATTENDEE;CN=Henry;PARTSTAT=NEEDS-ACTION:mailto:metahenry@metaprovide.org\r\n\
ATTENDEE;CN=Henry;PARTSTAT=ACCEPTED:mailto:metahenry@metaprovide.org\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn ignores_vtimezone_rrule() {
        let parsed = parse_ics(TZ_AND_EVENT, &[]).expect("parse");
        assert!(parsed.rrule.is_none(), "got {:?}", parsed.rrule);
        assert_eq!(parsed.summary, "Onboarding");
    }

    #[test]
    fn prefers_decided_partstat_over_needs_action() {
        let addrs = vec!["metahenry@metaprovide.org".into()];
        let parsed = parse_ics(TZ_AND_EVENT, &addrs).expect("parse");
        assert_eq!(parsed.my_partstat.as_deref(), Some("ACCEPTED"));
    }

    #[test]
    fn nogi_morning_keeps_wall_0600_across_dst() {
        const NOGI: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Pegla Schedule//EN\r\n\
BEGIN:VEVENT\r\n\
UID:32a6496a-5088-4610-87a6-3a338560edbc@pegla.local\r\n\
DTSTAMP:20260127T150000Z\r\n\
DTSTART;TZID=Europe/Stockholm:20260126T060000\r\n\
DTEND;TZID=Europe/Stockholm:20260126T070000\r\n\
SUMMARY:NOGI Morning\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";
        let parsed = parse_ics(NOGI, &[]).expect("parse");
        // Jan 26 winter CET UTC+1 => 05:00Z
        assert_eq!(
            parsed.dtstart.as_deref(),
            Some("2026-01-26T05:00:00+00:00"),
            "winter: {:?}",
            parsed.dtstart
        );
        let tz: Tz = "Europe/Stockholm".parse().unwrap();
        let range_start = "2026-08-17T00:00:00+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let range_end = "2026-08-17T23:59:59+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let occ = expand_rrule_from_raw(
            NOGI,
            parsed.dtstart.as_deref().unwrap(),
            parsed.rrule.as_deref().unwrap(),
            range_start,
            range_end,
        );
        assert_eq!(occ.len(), 1, "occ={:?}", occ);
        let local = occ[0].with_timezone(&tz);
        assert_eq!(
            local.format("%H:%M").to_string(),
            "06:00",
            "summer wall should stay 06:00, got {} (UTC {})",
            local.format("%Y-%m-%d %H:%M %Z"),
            occ[0]
        );
        // August 06:00 CEST => 04:00Z
        assert_eq!(occ[0].to_rfc3339(), "2026-08-17T04:00:00+00:00");

        // Check legacy UTC expansion would drift to 07:00
        let old = expand_rrule_occurrences(
            parsed.dtstart.as_deref().unwrap(),
            parsed.rrule.as_deref().unwrap(),
            range_start,
            range_end,
        );
        let old_local = old[0].with_timezone(&tz);
        assert_eq!(old_local.format("%H:%M").to_string(), "07:00");
    }

    #[test]
    fn floating_is_treated_as_stockholm() {
        const ICS: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:float-1\r\n\
DTSTAMP:20260801T120000Z\r\n\
DTSTART:20260914T090000\r\n\
DTEND:20260914T100000\r\n\
SUMMARY:Standup\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";
        let p = parse_ics(ICS, &[]).expect("parse");
        // Sep 14 CEST => 07:00Z
        assert_eq!(p.dtstart.as_deref(), Some("2026-09-14T07:00:00+00:00"));
    }

    #[test]
    fn exdate_filters_single_occurrence() {
        const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:ex-1\r\nDTSTAMP:20260126T150000Z\r\nDTSTART;TZID=Europe/Stockholm:20260126T060000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let parsed = parse_ics(ICS, &[]).expect("parse");
        let tz: Tz = "Europe/Stockholm".parse().unwrap();
        let range_start = "2026-08-10T00:00:00+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let range_end = "2026-08-24T23:59:59+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let occ_before = expand_rrule_from_raw(
            ICS,
            parsed.dtstart.as_deref().unwrap(),
            parsed.rrule.as_deref().unwrap(),
            range_start,
            range_end,
        );
        assert_eq!(occ_before.len(), 3); // 10, 17, 24
                                         // Inject EXDATE for Aug 17
        let occ_mid = "2026-08-17T04:00:00+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let raw2 = inject_exdate(ICS, occ_mid, Some(tz));
        let occ_after = expand_rrule_from_raw(
            &raw2,
            parsed.dtstart.as_deref().unwrap(),
            parsed.rrule.as_deref().unwrap(),
            range_start,
            range_end,
        );
        assert_eq!(occ_after.len(), 2);
        assert!(!occ_after
            .iter()
            .any(|d| d.timestamp() == occ_mid.timestamp()));
    }

    #[test]
    fn truncate_until_excludes_future() {
        const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:trunc-1\r\nDTSTAMP:20260126T150000Z\r\nDTSTART;TZID=Europe/Stockholm:20260126T060000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let parsed = parse_ics(ICS, &[]).expect("parse");
        let tz: Tz = "Europe/Stockholm".parse().unwrap();
        // Truncate before Aug 17 (so keep up to Aug 10) — use occurrence -1s
        let occ = "2026-08-17T04:00:00+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let until_wall2 = (occ - chrono::Duration::seconds(1))
            .with_timezone(&tz)
            .naive_local();
        let raw2 = truncate_rrule_until(ICS, until_wall2, Some(tz), false);
        assert!(raw2.contains("UNTIL="));
        let new_rrule = raw2
            .lines()
            .find(|l| l.to_uppercase().starts_with("RRULE"))
            .unwrap()
            .split(':')
            .nth(1)
            .unwrap()
            .to_string();
        let range_start = "2026-01-01T00:00:00+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let range_end = "2026-12-31T23:59:59+00:00"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let occ_after = expand_rrule_from_raw(
            &raw2,
            parsed.dtstart.as_deref().unwrap(),
            &new_rrule,
            range_start,
            range_end,
        );
        // Should not contain Aug 17 or later
        for o in &occ_after {
            assert!(*o < occ, "found future occ {}", o);
        }
        assert!(!occ_after.iter().any(|d| d.timestamp() == occ.timestamp()));
        assert!(occ_after
            .iter()
            .any(|d| d.with_timezone(&tz).format("%Y-%m-%d").to_string() == "2026-08-10"));
    }

    #[test]
    fn build_parse_roundtrip_preserves_uid_and_summary() {
        let input = EventInput {
            calendar_id: 1,
            uid: None,
            summary: "Test meeting".into(),
            description: "".into(),
            location: "".into(),
            dtstart: "2026-08-12".into(),
            dtend: "2026-08-13".into(),
            all_day: true,
            timezone: "Europe/Stockholm".into(),
            rrule: None,
            alarms: vec![crate::db::AlarmInfo {
                trigger: "-PT15M".into(),
                description: Some("Reminder".into()),
            }],
            attendees: vec![],
            href: None,
            etag: None,
        };
        let (uid, ics) = build_ics(&input, None).expect("build");
        assert!(
            ics.to_uppercase().contains("SUMMARY:TEST MEETING"),
            "ics={ics}"
        );
        assert!(ics.contains(&format!("UID:{uid}")), "ics={ics}");
        let parsed = parse_ics(&ics, &[]).expect("parse");
        assert_eq!(parsed.uid, uid);
        assert_eq!(parsed.summary, "Test meeting");
    }
}
