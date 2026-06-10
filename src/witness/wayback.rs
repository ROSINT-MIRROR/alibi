use super::{Snapshot, Witness, agent, short_err};
use crate::time_util;

pub const KEY_ENV: &str = "ARCHIVE_ORG_KEY";

pub struct Wayback {
    key: Option<String>,
}

impl Wayback {
    pub fn from_env() -> Self {
        Self {
            key: std::env::var(KEY_ENV)
                .ok()
                .map(|k| k.trim().to_string())
                .filter(|k| k.contains(':')),
        }
    }
}

impl Witness for Wayback {
    fn name(&self) -> &'static str {
        "wayback"
    }

    fn available(&self) -> Result<(), String> {
        match &self.key {
            Some(_) => Ok(()),
            // anon /save lane is dead (CLAUDE.md doctrine §3) — key or nothing
            None => Err(format!(
                "{KEY_ENV} not set — free key: https://archive.org/account/s3.php"
            )),
        }
    }

    fn submit(&self, url: &str) -> Result<String, String> {
        let key = self.key.as_deref().ok_or("no key")?;
        let resp = agent()
            .post("https://web.archive.org/save")
            .set("Accept", "application/json")
            .set("Authorization", &format!("LOW {key}"))
            .send_form(&[("url", url), ("capture_all", "1"), ("skip_first_archive", "1")])
            .map_err(short_err)?;
        let body = resp.into_string().map_err(|e| e.to_string())?;
        let job = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("job_id").and_then(|j| j.as_str()).map(String::from));
        Ok(job.map_or_else(|| "accepted".into(), |j| format!("job {j}")))
    }

    fn newest(&self, url: &str) -> Result<Option<Snapshot>, String> {
        let resp = agent()
            .get("https://web.archive.org/cdx/search/cdx")
            .query("url", url)
            .query("output", "json")
            .query("limit", "-1")
            .query("fl", "timestamp,original")
            .call()
            .map_err(short_err)?;
        let body = resp.into_string().map_err(|e| e.to_string())?;
        Ok(cdx_newest(&body).map(|(ts, orig)| Snapshot {
            url: format!("https://web.archive.org/web/{ts}/{orig}"),
            epoch: time_util::ts14_to_epoch(&ts),
            ts: Some(ts),
        }))
    }
}

pub(crate) fn cdx_newest(body: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let rows = v.as_array()?;
    // row 0 is the CDX header; limit=-1 means data rows are newest-last
    let row = rows.iter().skip(1).next_back()?;
    let ts = row.get(0)?.as_str()?.to_string();
    let orig = row.get(1)?.as_str()?.to_string();
    Some((ts, orig))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdx_parses_newest_row() {
        let body = r#"[["timestamp","original"],
["20260526193415","https://www.ksl.com/article/51374190"],
["20260526193504","https://www.ksl.com/article/51374190"]]"#;
        let (ts, orig) = cdx_newest(body).unwrap();
        assert_eq!(ts, "20260526193504");
        assert_eq!(orig, "https://www.ksl.com/article/51374190");
    }

    #[test]
    fn cdx_empty_and_garbage() {
        assert_eq!(cdx_newest("[]"), None);
        assert_eq!(cdx_newest(""), None);
        assert_eq!(cdx_newest("<html>503</html>"), None);
        // header only, no captures
        assert_eq!(cdx_newest(r#"[["timestamp","original"]]"#), None);
    }
}
