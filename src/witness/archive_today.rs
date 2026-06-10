use super::{Snapshot, Witness, agent, short_err};
use crate::time_util;

/// Mirror domains rotate as they get blocked; try in order.
const MIRRORS: &[&str] = &[
    "https://archive.ph",
    "https://archive.today",
    "https://archive.is",
    "https://archive.md",
];

pub struct ArchiveToday;

impl ArchiveToday {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ArchiveToday {
    fn default() -> Self {
        Self::new()
    }
}

impl Witness for ArchiveToday {
    fn name(&self) -> &'static str {
        "archivetoday"
    }

    fn submit(&self, url: &str) -> Result<String, String> {
        let mut last = String::from("no mirror reachable");
        for m in MIRRORS {
            // one agent per mirror: the homepage GET sets session cookies the
            // POST must carry, and serves the optional submitid anti-bot token
            let session = agent();
            let submitid = session
                .get(m)
                .call()
                .ok()
                .and_then(|r| r.into_string().ok())
                .and_then(|html| find_submitid(&html));
            let req = session.post(&format!("{m}/submit/")).set("Referer", m);
            let res = match &submitid {
                Some(id) => req.send_form(&[("url", url), ("submitid", id)]),
                None => req.send_form(&[("url", url)]),
            };
            match res {
                Ok(_) => return Ok(format!("via {m}")),
                // HTTP refusal (429/403): every mirror fronts the same backend,
                // rotating just hammers the same rate limiter — stop here
                Err(e @ ureq::Error::Status(..)) => return Err(format!("{m}: {}", short_err(e))),
                // transport errors (DNS, connect): the domain may be blocked, rotate
                Err(e) => last = format!("{m}: {}", short_err(e)),
            }
        }
        Err(last)
    }

    fn newest(&self, url: &str) -> Result<Option<Snapshot>, String> {
        // archive.today carries troll mementos with future dates (e.g. 2035 on
        // example.com); an uncapped max would shadow real captures forever
        let cap = time_util::epoch_now() + 2 * 24 * 3600;
        let mut last = String::from("no mirror reachable");
        for m in MIRRORS {
            // NB: archive.today serves link-format at /timemap/<url> — there is
            // no /timemap/link/ prefix (that 404s), unlike IA's memento layout
            match agent().get(&format!("{m}/timemap/{url}")).call() {
                Ok(resp) => {
                    let body = resp.into_string().map_err(|e| e.to_string())?;
                    return Ok(timemap_newest(&body, cap).map(|(u, dt)| Snapshot {
                        epoch: time_util::rfc1123_to_epoch(&dt),
                        ts: Some(dt),
                        url: u,
                    }));
                }
                // 404 = no mementos for this URL: an answer, not an error
                Err(ureq::Error::Status(404, _)) => return Ok(None),
                Err(e) => last = format!("{m}: {}", short_err(e)),
            }
        }
        Err(last)
    }
}

pub(crate) fn find_submitid(html: &str) -> Option<String> {
    let i = html.find("name=\"submitid\"")?;
    let window = &html[i..(i + 400).min(html.len())];
    let v = window.find("value=\"")? + 7;
    let rest = &window[v..];
    let end = rest.find('"')?;
    let id = &rest[..end];
    (!id.is_empty()).then(|| id.to_string())
}

/// Link-format timemap, one entry per line. Freshest memento dated ≤ `cap`
/// wins; if every memento is future-dated, fall back to the absolute max so
/// `check` still reports what the public record shows.
pub(crate) fn timemap_newest(body: &str, cap: u64) -> Option<(String, String)> {
    let mut best: Option<(String, String, u64)> = None;
    let mut best_any: Option<(String, String, u64)> = None;
    for line in body.lines() {
        if !line.contains("memento") || !line.contains("datetime=\"") {
            continue;
        }
        let Some(a) = line.find('<') else { continue };
        let Some(b) = line[a + 1..].find('>') else { continue };
        let url = &line[a + 1..a + 1 + b];
        let Some(d) = line.find("datetime=\"") else { continue };
        let rest = &line[d + 10..];
        let Some(e) = rest.find('"') else { continue };
        let dt = &rest[..e];
        let epoch = time_util::rfc1123_to_epoch(dt).unwrap_or(0);
        if best_any.as_ref().is_none_or(|(_, _, be)| epoch >= *be) {
            best_any = Some((url.to_string(), dt.to_string(), epoch));
        }
        if epoch <= cap && best.as_ref().is_none_or(|(_, _, be)| epoch >= *be) {
            best = Some((url.to_string(), dt.to_string(), epoch));
        }
    }
    best.or(best_any).map(|(u, d, _)| (u, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEMAP: &str = r#"<http://example.com/>; rel="original",
<https://archive.ph/timemap/http://example.com/>; rel="self"; type="application/link-format"; from="Sat, 12 Jul 2014 09:30:46 GMT",
<https://archive.ph/timegate/http://example.com/>; rel="timegate",
<https://archive.ph/20140712093046/http://example.com/>; rel="first memento"; datetime="Sat, 12 Jul 2014 09:30:46 GMT",
<https://archive.ph/20251225120000/http://example.com/>; rel="memento"; datetime="Thu, 25 Dec 2025 12:00:00 GMT",
<https://archive.ph/20260101000000/http://example.com/>; rel="last memento"; datetime="Thu, 01 Jan 2026 00:00:00 GMT"
"#;

    // far past any test-relevant date; lets "no cap" cases pick the true max
    const NO_CAP: u64 = u64::MAX;

    #[test]
    fn timemap_picks_freshest() {
        let (url, dt) = timemap_newest(TIMEMAP, NO_CAP).unwrap();
        assert_eq!(url, "https://archive.ph/20260101000000/http://example.com/");
        assert_eq!(dt, "Thu, 01 Jan 2026 00:00:00 GMT");
    }

    #[test]
    fn timemap_cap_skips_troll_mementos() {
        let troll = format!(
            "{TIMEMAP},\n<https://archive.ph/20351231145959/http://example.com/>; rel=\"last memento\"; datetime=\"Mon, 31 Dec 2035 14:59:59 GMT\"\n"
        );
        // cap at mid-2026: the 2035 memento must not shadow the real newest
        let cap = crate::time_util::ts14_to_epoch("20260701000000").unwrap();
        let (url, _) = timemap_newest(&troll, cap).unwrap();
        assert_eq!(url, "https://archive.ph/20260101000000/http://example.com/");
        // but if every memento is future-dated, fall back to reporting it
        let only_troll = r#"<https://archive.ph/20351231145959/http://x/>; rel="memento"; datetime="Mon, 31 Dec 2035 14:59:59 GMT""#;
        assert!(timemap_newest(only_troll, cap).is_some());
    }

    #[test]
    fn timemap_empty() {
        assert_eq!(timemap_newest("", NO_CAP), None);
        assert_eq!(timemap_newest("<h1>404</h1>", NO_CAP), None);
    }

    #[test]
    fn submitid_extraction() {
        let html = r#"<input type="hidden" name="submitid" value="abc123XYZ"/>"#;
        assert_eq!(find_submitid(html), Some("abc123XYZ".into()));
        assert_eq!(find_submitid("<form></form>"), None);
        // empty value attribute = no token
        assert_eq!(find_submitid(r#"name="submitid" value="""#), None);
    }
}
