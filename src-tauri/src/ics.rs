use crate::db::{AlarmInfo, AttendeeInfo};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use icalendar::{Calendar, CalendarComponent, Component, DatePerhapsTime, Event, EventLike, EventStatus};
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

pub fn parse_ics(raw: &str, my_addresses: &[String]) -> Option<ParsedEvent> {
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
        Some(s) => date_perhaps_to_strings(s),
        None => extract_dt_from_raw(raw, "DTSTART"),
    };
    let (dtend, _) = match event.get_end() {
        Some(s) => date_perhaps_to_strings(s),
        None => extract_dt_from_raw(raw, "DTEND"),
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

fn emails_equal(a: &str, b: &str) -> bool {
    let na = a.trim().trim_start_matches("mailto:").to_lowercase();
    let nb = b.trim().trim_start_matches("mailto:").to_lowercase();
    na == nb
}

fn date_perhaps_to_strings(dt: DatePerhapsTime) -> (Option<String>, bool) {
    match dt {
        DatePerhapsTime::Date(d) => (Some(d.format("%Y-%m-%d").to_string()), true),
        DatePerhapsTime::DateTime(cdt) => calendar_date_time_to_strings(cdt),
    }
}

fn calendar_date_time_to_strings(dt: icalendar::CalendarDateTime) -> (Option<String>, bool) {
    use icalendar::CalendarDateTime as CDT;
    match dt {
        CDT::Floating(ndt) => (
            Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc).to_rfc3339()),
            false,
        ),
        CDT::Utc(dt) => (Some(dt.to_rfc3339()), false),
        CDT::WithTimezone { date_time, tzid } => {
            if let Ok(tz) = tzid.parse::<Tz>() {
                if let Some(ldt) = tz.from_local_datetime(&date_time).single() {
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

fn extract_dt_from_raw(raw: &str, name: &str) -> (Option<String>, bool) {
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
                return (
                    Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc).to_rfc3339()),
                    false,
                );
            }
        }
    }
    (None, false)
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
            params.insert(
                k.to_uppercase(),
                v.trim_matches('"').to_string(),
            );
        }
    }
    (params, value)
}

pub fn build_ics(input: &EventInput, existing_uid: Option<&str>) -> Result<(String, String), String> {
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
        let s = NaiveDate::parse_from_str(&input.dtstart[..10.min(input.dtstart.len())], "%Y-%m-%d")
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

pub fn alarm_trigger_at(
    event_start: DateTime<Utc>,
    trigger: &str,
) -> Option<DateTime<Utc>> {
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
        assert_eq!(p.dtstart.as_deref(), Some("2026-09-14T09:00:00+00:00"));
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
