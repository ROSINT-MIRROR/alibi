use super::{Snapshot, Witness, agent, short_err};

const BASE: &str = "https://ghostarchive.org";

/// Endpoints probed live 2026-06-10:
/// `<form action="/archive2" method="POST"><input name="archive">` and
/// `/search?term=<url>` results as `href="/archive/<id>"`, newest data unknown —
/// search exposes no dates, so freshness falls back to id-diff in establish().
pub struct GhostArchive;

impl GhostArchive {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GhostArchive {
    fn default() -> Self {
        Self::new()
    }
}

impl Witness for GhostArchive {
    fn name(&self) -> &'static str {
        "ghostarchive"
    }

    fn submit(&self, url: &str) -> Result<String, String> {
        agent()
            .post(&format!("{BASE}/archive2"))
            .set("Referer", BASE)
            .send_form(&[("archive", url)])
            .map(|_| "accepted".to_string())
            .map_err(short_err)
    }

    fn newest(&self, url: &str) -> Result<Option<Snapshot>, String> {
        let resp = agent()
            .get(&format!("{BASE}/search"))
            .query("term", url)
            .call()
            .map_err(short_err)?;
        let body = resp.into_string().map_err(|e| e.to_string())?;
        Ok(first_archive_link(&body).map(|path| Snapshot {
            url: format!("{BASE}{path}"),
            ts: None,
            epoch: None,
        }))
    }
}

pub(crate) fn first_archive_link(html: &str) -> Option<String> {
    let i = html.find("href=\"/archive/")?;
    let rest = &html[i + 6..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_first_archive_href() {
        let html = r#"<a href="/search?term=x&amp;page=1">next</a>
<a href="/archive/2pWJE">hit</a><a href="/archive/5UOyX">hit2</a>"#;
        assert_eq!(first_archive_link(html), Some("/archive/2pWJE".into()));
    }

    #[test]
    fn no_results() {
        assert_eq!(first_archive_link("<html>nothing here</html>"), None);
    }
}
