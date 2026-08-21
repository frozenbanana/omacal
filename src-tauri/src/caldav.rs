use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone)]
pub struct CalDavClient {
    http: reqwest::Client,
    base_url: String,
    username: String,
    password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCalendar {
    pub href: String,
    pub displayname: String,
    pub color: Option<String>,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
    pub readonly: bool,
}

#[derive(Debug, Clone)]
pub struct RemoteObject {
    pub href: String,
    pub etag: Option<String>,
    pub data: Option<String>,
}

impl CalDavClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("Omacal/0.1")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;
        Ok(Self {
            http,
            base_url: normalize_caldav_url(base_url),
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let cred = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", self.username, self.password),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {cred}")).unwrap(),
        );
        headers
    }

    fn resolve(&self, href: &str) -> String {
        if href.starts_with("http://") || href.starts_with("https://") {
            href.to_string()
        } else if href.starts_with('/') {
            // keep host from base
            let url = url::Url::parse(&self.base_url).unwrap();
            let host = match url.port() {
                Some(p) => format!("{}:{p}", url.host_str().unwrap_or("")),
                None => url.host_str().unwrap_or("").to_string(),
            };
            format!("{}://{host}{href}", url.scheme())
        } else {
            format!("{}/{}", self.base_url, href.trim_start_matches('/'))
        }
    }

    async fn request(
        &self,
        method: Method,
        url: &str,
        body: Option<String>,
        extra: &[(&str, &str)],
    ) -> Result<reqwest::Response> {
        use reqwest::header::HeaderName;
        let mut headers = self.auth_headers();
        for (k, v) in extra {
            headers.insert(
                HeaderName::from_bytes(k.as_bytes())?,
                HeaderValue::from_str(v)?,
            );
        }
        let mut builder = self.http.request(method, url).headers(headers);
        if let Some(b) = body {
            builder = builder.body(b);
        }
        let resp = builder.send().await?;
        Ok(resp)
    }

    pub async fn discover_principal(&self) -> Result<String> {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop><d:current-user-principal/></d:prop>
</d:propfind>"#;

        // Try base + well-known; Nextcloud answers on /remote.php/dav/
        let mut candidates = vec![
            format!("{}/", self.base_url.trim_end_matches('/')),
            self.base_url.trim_end_matches('/').to_string(),
        ];
        if let Ok(u) = url::Url::parse(&self.base_url) {
            candidates.push(format!(
                "{}://{}/.well-known/caldav",
                u.scheme(),
                u.host_str().unwrap_or("")
            ));
        }

        let mut last_err = String::new();
        for url in candidates {
            let resp = match self
                .request(
                    Method::from_bytes(b"PROPFIND")?,
                    &url,
                    Some(body.to_string()),
                    &[
                        ("Depth", "0"),
                        ("Content-Type", "application/xml; charset=utf-8"),
                    ],
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = format!("{url}: {e}");
                    continue;
                }
            };
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(anyhow!(
                    "authentication failed ({status}). Check username and app password."
                ));
            }
            if !status.is_success() && status.as_u16() != 207 {
                last_err = format!("{url} -> {status}: {}", truncate(&text, 240));
                continue;
            }
            if let Some(href) = extract_href_prop(&text, "current-user-principal") {
                return Ok(href);
            }
            // Some servers put principal href at top-level only after auth — keep trying
            last_err = format!(
                "{url} -> {status}, no principal in XML (prefix?): {}",
                truncate(&text, 320)
            );
        }
        Err(anyhow!(
            "no current-user-principal in response ({last_err})"
        ))
    }

    pub async fn discover_calendar_home(&self, principal: &str) -> Result<String> {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop><c:calendar-home-set/></d:prop>
</d:propfind>"#;
        let url = self.resolve(principal);
        let resp = self
            .request(
                Method::from_bytes(b"PROPFIND")?,
                &url,
                Some(body.into()),
                &[
                    ("Depth", "0"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
            )
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() && status.as_u16() != 207 {
            return Err(anyhow!("calendar-home discovery failed: {status} {text}"));
        }
        extract_href_prop(&text, "calendar-home-set")
            .ok_or_else(|| anyhow!("no calendar-home-set in response"))
    }

    pub async fn list_calendars(&self, home: &str) -> Result<Vec<RemoteCalendar>> {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:x1="http://apple.com/ns/ical/">
  <d:prop>
    <d:displayname/>
    <d:resourcetype/>
    <d:current-user-privilege-set/>
    <cs:getctag/>
    <d:sync-token/>
    <x1:calendar-color/>
    <c:supported-calendar-component-set/>
  </d:prop>
</d:propfind>"#;
        let url = self.resolve(home);
        let resp = self
            .request(
                Method::from_bytes(b"PROPFIND")?,
                &url,
                Some(body.into()),
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
            )
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() && status.as_u16() != 207 {
            return Err(anyhow!("list calendars failed: {status} {text}"));
        }
        Ok(parse_calendar_list(&text))
    }

    pub async fn sync_collection(
        &self,
        calendar_href: &str,
        sync_token: Option<&str>,
    ) -> Result<(Vec<RemoteObject>, Option<String>)> {
        let token_xml = match sync_token {
            Some(t) if !t.is_empty() => format!("<d:sync-token>{t}</d:sync-token>"),
            _ => String::new(),
        };
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8" ?>
<d:sync-collection xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  {token_xml}
  <d:sync-level>1</d:sync-level>
  <d:prop>
    <d:getetag/>
    <c:calendar-data/>
  </d:prop>
</d:sync-collection>"#
        );
        let url = self.resolve(calendar_href);
        let resp = self
            .request(
                Method::from_bytes(b"REPORT")?,
                &url,
                Some(body),
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
            )
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if status.as_u16() == 403 || status.as_u16() == 400 {
            // fall back to full query
            return self.calendar_query_all(calendar_href).await;
        }
        if !status.is_success() && status.as_u16() != 207 {
            return Err(anyhow!("sync-collection failed: {status} {text}"));
        }
        let objects = parse_objects(&text);
        let new_token = extract_text_prop(&text, "sync-token");
        Ok((objects, new_token))
    }

    pub async fn calendar_query_all(
        &self,
        calendar_href: &str,
    ) -> Result<(Vec<RemoteObject>, Option<String>)> {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:getetag/>
    <c:calendar-data/>
  </d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT"/>
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#;
        let url = self.resolve(calendar_href);
        let resp = self
            .request(
                Method::from_bytes(b"REPORT")?,
                &url,
                Some(body.into()),
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
            )
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() && status.as_u16() != 207 {
            return Err(anyhow!("calendar-query failed: {status} {text}"));
        }
        // also fetch sync-token/ctag
        let meta = self.propfind_meta(calendar_href).await.unwrap_or_default();
        Ok((parse_objects(&text), meta.get("sync-token").cloned()))
    }

    pub async fn propfind_meta(&self, href: &str) -> Result<HashMap<String, String>> {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:" xmlns:cs="http://calendarserver.org/ns/">
  <d:prop>
    <d:sync-token/>
    <cs:getctag/>
  </d:prop>
</d:propfind>"#;
        let url = self.resolve(href);
        let resp = self
            .request(
                Method::from_bytes(b"PROPFIND")?,
                &url,
                Some(body.into()),
                &[
                    ("Depth", "0"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
            )
            .await?;
        let text = resp.text().await?;
        let mut map = HashMap::new();
        if let Some(t) = extract_text_prop(&text, "sync-token") {
            map.insert("sync-token".into(), t);
        }
        if let Some(t) = extract_text_prop(&text, "getctag") {
            map.insert("ctag".into(), t);
        }
        Ok(map)
    }

    pub async fn put_object(
        &self,
        href: &str,
        ics: &str,
        etag: Option<&str>,
    ) -> Result<Option<String>> {
        let url = self.resolve(href);
        let mut headers = self.auth_headers();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/calendar; charset=utf-8"),
        );
        if let Some(e) = etag {
            let v = format!("\"{}\"", e.trim_matches('"'));
            headers.insert("If-Match", HeaderValue::from_str(&v)?);
        }
        let resp = self
            .http
            .put(&url)
            .headers(headers)
            .body(ics.to_string())
            .send()
            .await?;
        let status = resp.status();
        let new_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());
        if !(status.is_success() || status.as_u16() == 201 || status.as_u16() == 204) {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("PUT failed: {status} {text}"));
        }
        Ok(new_etag)
    }

    pub async fn delete_object(&self, href: &str, etag: Option<&str>) -> Result<()> {
        let url = self.resolve(href);
        let mut headers = self.auth_headers();
        if let Some(e) = etag {
            let v = format!("\"{}\"", e.trim_matches('"'));
            headers.insert("If-Match", HeaderValue::from_str(&v)?);
        }
        let resp = self.http.delete(&url).headers(headers).send().await?;
        let status = resp.status();
        if !(status.is_success() || status.as_u16() == 204 || status.as_u16() == 404) {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("DELETE failed: {status} {text}"));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_object(&self, href: &str) -> Result<RemoteObject> {
        let url = self.resolve(href);
        let resp = self
            .request(Method::GET, &url, None, &[("Accept", "text/calendar")])
            .await?;
        let status = resp.status();
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());
        let data = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("GET failed: {status} {data}"));
        }
        Ok(RemoteObject {
            href: href.to_string(),
            etag,
            data: Some(data),
        })
    }

    /// Free/busy REPORT (best-effort; Nextcloud may support)
    pub async fn freebusy(
        &self,
        home: &str,
        start: &str,
        end: &str,
        attendees: &[String],
    ) -> Result<String> {
        let attendee_xml: String = attendees
            .iter()
            .map(|a| {
                let mail = if a.starts_with("mailto:") {
                    a.clone()
                } else {
                    format!("mailto:{a}")
                };
                format!("ATTENDEE:{mail}\\n")
            })
            .collect();
        let ics = format!(
            "BEGIN:VCALENDAR\\nVERSION:2.0\\nBEGIN:VFREEBUSY\\nDTSTART:{start}\\nDTEND:{end}\\n{attendee_xml}END:VFREEBUSY\\nEND:VCALENDAR\\n"
        );
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8" ?>
<c:free-busy-query xmlns:c="urn:ietf:params:xml:ns:caldav">
  <c:time-range start="{start}" end="{end}"/>
</c:free-busy-query>"#
        );
        let _ = ics;
        let url = self.resolve(home);
        let resp = self
            .request(
                Method::from_bytes(b"REPORT")?,
                &url,
                Some(body),
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
            )
            .await
            .context("freebusy")?;
        Ok(resp.text().await?)
    }
}

fn normalize_caldav_url(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if !s.starts_with("http://") && !s.starts_with("https://") {
        s = format!("https://{s}");
    }
    // Drop trailing slash for storage; callers re-add when needed
    while s.ends_with('/') {
        s.pop();
    }
    s
}

fn truncate(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    if flat.len() <= max {
        flat
    } else {
        format!("{}…", &flat[..max])
    }
}

fn local_name(name: &[u8]) -> String {
    // Handles both Clark notation `{DAV:}href` and prefixed `d:href`
    let s = String::from_utf8_lossy(name);
    let after_brace = s.rsplit('}').next().unwrap_or(&s);
    after_brace
        .rsplit(':')
        .next()
        .unwrap_or(after_brace)
        .to_string()
}

fn extract_href_prop(xml: &str, prop: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_prop = false;
    let mut in_href = false;
    let mut text = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(e)) => {
                let n = local_name(e.name().as_ref());
                if n.eq_ignore_ascii_case(prop) {
                    in_prop = true;
                } else if in_prop && n.eq_ignore_ascii_case("href") {
                    in_href = true;
                    text.clear();
                }
            }
            Ok(XmlEvent::Text(t)) if in_href => {
                text.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(XmlEvent::End(e)) => {
                let n = local_name(e.name().as_ref());
                if n.eq_ignore_ascii_case("href") && in_href {
                    in_href = false;
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
                if n.eq_ignore_ascii_case(prop) {
                    in_prop = false;
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nextcloud_principal_xml() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/remote.php/dav/</d:href>
    <d:propstat>
      <d:prop>
        <d:current-user-principal>
          <d:href>/remote.php/dav/principals/users/henry.bergstrom/</d:href>
        </d:current-user-principal>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
        assert_eq!(
            extract_href_prop(xml, "current-user-principal").as_deref(),
            Some("/remote.php/dav/principals/users/henry.bergstrom/")
        );
    }

    #[test]
    fn parses_nextcloud_calendar_list_with_empty_tags() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:x1="http://apple.com/ns/ical/">
 <d:response>
  <d:href>/remote.php/dav/calendars/henry.bergstrom/</d:href>
  <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/henry.bergstrom/personal/</d:href>
  <d:propstat>
   <d:prop>
    <d:displayname>Personal</d:displayname>
    <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
    <d:current-user-privilege-set><d:privilege><d:write/></d:privilege></d:current-user-privilege-set>
    <cs:getctag>http://sabre.io/ns/sync/1</cs:getctag>
    <x1:calendar-color>#FF0000FF</x1:calendar-color>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/henry.bergstrom/work/</d:href>
  <d:propstat>
   <d:prop>
    <d:displayname>Work</d:displayname>
    <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
   </d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
 <d:response>
  <d:href>/remote.php/dav/calendars/henry.bergstrom/inbox/</d:href>
  <d:propstat>
   <d:prop><d:resourcetype><d:collection/><cal:schedule-inbox/></d:resourcetype></d:prop>
   <d:status>HTTP/1.1 200 OK</d:status>
  </d:propstat>
 </d:response>
</d:multistatus>"#;
        let cals = parse_calendar_list(xml);
        assert_eq!(
            cals.len(),
            2,
            "got: {:?}",
            cals.iter().map(|c| &c.displayname).collect::<Vec<_>>()
        );
        assert_eq!(cals[0].displayname, "Personal");
        assert_eq!(cals[1].displayname, "Work");
        assert_eq!(cals[0].color.as_deref(), Some("#FF0000"));
    }

    #[test]
    fn parses_fixture_from_live_nextcloud() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/cal_list_fixture.xml");
        let Ok(xml) = std::fs::read_to_string(path) else {
            return; // optional fixture
        };
        let cals = parse_calendar_list(&xml);
        assert!(
            cals.len() >= 3,
            "expected several calendars, got {}: {:?}",
            cals.len(),
            cals.iter().map(|c| &c.displayname).collect::<Vec<_>>()
        );
    }
}

fn extract_text_prop(xml: &str, prop: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_prop = false;
    let mut text = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(e)) => {
                let n = local_name(e.name().as_ref());
                if n.eq_ignore_ascii_case(prop) {
                    in_prop = true;
                    text.clear();
                }
            }
            Ok(XmlEvent::Text(t)) if in_prop => {
                text.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(XmlEvent::CData(t)) if in_prop => {
                text.push_str(&String::from_utf8_lossy(&t.into_inner()));
            }
            Ok(XmlEvent::End(e)) => {
                let n = local_name(e.name().as_ref());
                if n.eq_ignore_ascii_case(prop) && in_prop {
                    if !text.is_empty() {
                        return Some(text);
                    }
                    in_prop = false;
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn parse_calendar_list(xml: &str) -> Vec<RemoteCalendar> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut calendars = Vec::new();

    let mut in_response = false;
    let mut href = String::new();
    let mut displayname = String::new();
    let mut color = None;
    let mut ctag = None;
    let mut sync_token = None;
    let mut is_calendar = false;
    let mut is_collection = false;
    let mut has_vevent = false;
    let mut readonly = false;
    let mut text = String::new();
    let mut in_resourcetype = false;
    let mut in_priv = false;
    let mut in_comp_set = false;
    let mut has_write = false;
    let mut href_count = 0;

    let on_tag = |n: &str,
                  in_response: &mut bool,
                  in_resourcetype: &mut bool,
                  in_priv: &mut bool,
                  in_comp_set: &mut bool,
                  is_calendar: &mut bool,
                  is_collection: &mut bool,
                  has_write: &mut bool,
                  href: &mut String,
                  displayname: &mut String,
                  color: &mut Option<String>,
                  ctag: &mut Option<String>,
                  sync_token: &mut Option<String>,
                  readonly: &mut bool,
                  has_vevent: &mut bool,
                  href_count: &mut i32| {
        match n {
            "response" => {
                *in_response = true;
                href.clear();
                displayname.clear();
                *color = None;
                *ctag = None;
                *sync_token = None;
                *is_calendar = false;
                *is_collection = false;
                *has_vevent = false;
                *readonly = false;
                *has_write = false;
                *href_count = 0;
            }
            "resourcetype" => *in_resourcetype = true,
            "calendar" if *in_resourcetype => *is_calendar = true,
            "collection" if *in_resourcetype => *is_collection = true,
            "current-user-privilege-set" => *in_priv = true,
            "write" | "write-content" | "write-properties" | "all" if *in_priv => {
                *has_write = true;
            }
            "supported-calendar-component-set" => *in_comp_set = true,
            _ => {}
        }
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(e)) => {
                let n = local_name(e.name().as_ref());
                text.clear();
                // comp name attribute: <c:comp name="VEVENT"/>
                if in_comp_set && n.eq_ignore_ascii_case("comp") {
                    for a in e.attributes().flatten() {
                        if local_name(a.key.as_ref()).eq_ignore_ascii_case("name") {
                            let v = a.unescape_value().unwrap_or_default();
                            if v.eq_ignore_ascii_case("VEVENT") {
                                has_vevent = true;
                            }
                        }
                    }
                }
                on_tag(
                    &n,
                    &mut in_response,
                    &mut in_resourcetype,
                    &mut in_priv,
                    &mut in_comp_set,
                    &mut is_calendar,
                    &mut is_collection,
                    &mut has_write,
                    &mut href,
                    &mut displayname,
                    &mut color,
                    &mut ctag,
                    &mut sync_token,
                    &mut readonly,
                    &mut has_vevent,
                    &mut href_count,
                );
            }
            Ok(XmlEvent::Empty(e)) => {
                // Nextcloud uses self-closing tags: <c:calendar/>, <d:collection/>, <d:write/>
                let n = local_name(e.name().as_ref());
                if in_comp_set && n.eq_ignore_ascii_case("comp") {
                    for a in e.attributes().flatten() {
                        if local_name(a.key.as_ref()).eq_ignore_ascii_case("name") {
                            let v = a.unescape_value().unwrap_or_default();
                            if v.eq_ignore_ascii_case("VEVENT") {
                                has_vevent = true;
                            }
                        }
                    }
                }
                on_tag(
                    &n,
                    &mut in_response,
                    &mut in_resourcetype,
                    &mut in_priv,
                    &mut in_comp_set,
                    &mut is_calendar,
                    &mut is_collection,
                    &mut has_write,
                    &mut href,
                    &mut displayname,
                    &mut color,
                    &mut ctag,
                    &mut sync_token,
                    &mut readonly,
                    &mut has_vevent,
                    &mut href_count,
                );
            }
            Ok(XmlEvent::Text(t)) => {
                text.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(XmlEvent::CData(t)) => {
                text.push_str(&String::from_utf8_lossy(&t.into_inner()));
            }
            Ok(XmlEvent::End(e)) => {
                let n = local_name(e.name().as_ref());
                if in_response {
                    match n.as_str() {
                        "href" => {
                            // First href in a response is the resource path
                            if href_count == 0 {
                                href = text.clone();
                            }
                            href_count += 1;
                        }
                        "displayname" => displayname = text.clone(),
                        "calendar-color" => {
                            let c = text.trim();
                            if !c.is_empty() {
                                let hex = if c.starts_with('#') && c.len() >= 7 {
                                    c[..7].to_string()
                                } else {
                                    c.to_string()
                                };
                                color = Some(hex);
                            }
                        }
                        "getctag" => ctag = Some(text.clone()),
                        "sync-token" => sync_token = Some(text.clone()),
                        "resourcetype" => in_resourcetype = false,
                        "supported-calendar-component-set" => in_comp_set = false,
                        "current-user-privilege-set" => {
                            in_priv = false;
                            readonly = !has_write;
                        }
                        "response" => {
                            // Accept CalDAV calendars; also VEVENT-capable collections
                            let accept = (is_calendar || has_vevent)
                                && !href.is_empty()
                                && !href.contains("/inbox")
                                && !href.contains("/outbox");
                            if accept {
                                let name = if displayname.is_empty() {
                                    href.trim_end_matches('/')
                                        .rsplit('/')
                                        .next()
                                        .unwrap_or("calendar")
                                        .to_string()
                                } else {
                                    displayname.clone()
                                };
                                calendars.push(RemoteCalendar {
                                    href: href.clone(),
                                    displayname: name,
                                    color: color.take(),
                                    ctag: ctag.take(),
                                    sync_token: sync_token.take(),
                                    readonly,
                                });
                            }
                            let _ = is_collection;
                            in_response = false;
                        }
                        _ => {}
                    }
                }
                text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    calendars
}

fn parse_objects(xml: &str) -> Vec<RemoteObject> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = Vec::new();

    let mut in_response = false;
    let mut href = String::new();
    let mut etag = None;
    let mut data = None;
    let mut status_404 = false;
    let mut in_tag = String::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(e)) => {
                let n = local_name(e.name().as_ref());
                in_tag = n.clone();
                text.clear();
                if n == "response" {
                    in_response = true;
                    href.clear();
                    etag = None;
                    data = None;
                    status_404 = false;
                }
            }
            Ok(XmlEvent::Text(t)) => {
                text.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(XmlEvent::CData(t)) => {
                text.push_str(&String::from_utf8_lossy(&t.into_inner()));
            }
            Ok(XmlEvent::End(e)) => {
                let n = local_name(e.name().as_ref());
                if in_response {
                    match n.as_str() {
                        "href" if href.is_empty() => href = text.clone(),
                        "getetag" => {
                            etag = Some(text.trim().trim_matches('"').to_string());
                        }
                        "calendar-data" => data = Some(text.clone()),
                        "status" => {
                            if text.contains("404") {
                                status_404 = true;
                            }
                        }
                        "response" => {
                            if !href.is_empty() {
                                if status_404 {
                                    out.push(RemoteObject {
                                        href: href.clone(),
                                        etag: None,
                                        data: None, // deleted
                                    });
                                } else if data.is_some() {
                                    out.push(RemoteObject {
                                        href: href.clone(),
                                        etag: etag.take(),
                                        data: data.take(),
                                    });
                                }
                            }
                            in_response = false;
                        }
                        _ => {}
                    }
                }
                let _ = in_tag;
                text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}
