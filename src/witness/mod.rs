pub mod archive_today;
pub mod ghostarchive;
pub mod wayback;

use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub url: String,
    /// witness-native timestamp string when known (ts14 or RFC1123)
    pub ts: Option<String>,
    pub epoch: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Confirmed,
    Pending,
    Absent,
    Skipped,
    Submitted,
    Error,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Confirmed => "confirmed",
            Status::Pending => "pending",
            Status::Absent => "none",
            Status::Skipped => "skipped",
            Status::Submitted => "submitted",
            Status::Error => "error",
        }
    }

    /// Submitted is informational; everything else ends a witness's run.
    pub fn is_terminal(self) -> bool {
        !matches!(self, Status::Submitted)
    }
}

#[derive(Clone, Debug)]
pub struct Testimony {
    pub witness: &'static str,
    pub url: String,
    pub status: Status,
    pub snapshot: Option<Snapshot>,
    pub note: Option<String>,
}

impl Testimony {
    fn new(witness: &'static str, url: &str, status: Status) -> Self {
        Self { witness, url: url.to_string(), status, snapshot: None, note: None }
    }

    pub fn confirmed(witness: &'static str, url: &str, snapshot: Snapshot) -> Self {
        Self { snapshot: Some(snapshot), ..Self::new(witness, url, Status::Confirmed) }
    }

    pub fn pending(witness: &'static str, url: &str) -> Self {
        Self::new(witness, url, Status::Pending)
    }

    pub fn absent(witness: &'static str, url: &str) -> Self {
        Self::new(witness, url, Status::Absent)
    }

    pub fn skipped(witness: &'static str, url: &str, reason: String) -> Self {
        Self { note: Some(reason), ..Self::new(witness, url, Status::Skipped) }
    }

    pub fn submitted(witness: &'static str, url: &str, note: String) -> Self {
        Self { note: Some(note), ..Self::new(witness, url, Status::Submitted) }
    }

    pub fn error(witness: &'static str, url: &str, reason: String) -> Self {
        Self { note: Some(reason), ..Self::new(witness, url, Status::Error) }
    }

    /// Stable machine code a calling tool can branch on without parsing prose.
    /// Derived from status + the concrete failure detail (`note`):
    /// `ok` confirmed · `absent` nothing archived · `no_key`/`skipped` not attempted ·
    /// `rate_limited` 429 (retry after cooldown / torify) · `server_error` 5xx ·
    /// `timeout` slow read/connect · `network` DNS/connect/TLS · `not_yet` submitted
    /// fine but not yet public at timeout (just keep polling) · `error` other.
    pub fn reason(&self) -> &'static str {
        match self.status {
            Status::Confirmed => "ok",
            Status::Absent => "absent",
            Status::Submitted => "submitted",
            Status::Skipped => {
                if self.note.as_deref().is_some_and(|n| n.contains("ARCHIVE_ORG_KEY")) {
                    "no_key"
                } else {
                    "skipped"
                }
            }
            Status::Pending | Status::Error => match self.note.as_deref() {
                None => "not_yet", // pending with no recorded failure = submitted ok, just slow
                Some(n) => classify_detail(n),
            },
        }
    }
}

/// Map a raw failure string (our own `http {code}` / ureq messages) to a code.
fn classify_detail(msg: &str) -> &'static str {
    let m = msg.to_ascii_lowercase();
    if m.contains("429") {
        "rate_limited"
    } else if ["500", "502", "503", "504"].iter().any(|c| m.contains(c)) {
        "server_error"
    } else if m.contains("timed out") || m.contains("timeout") {
        "timeout"
    } else if m.contains("dns") || m.contains("connect") || m.contains("tls") || m.contains("network") {
        "network"
    } else {
        "error"
    }
}

pub trait Witness: Send + Sync {
    fn name(&self) -> &'static str;

    /// Err = human-readable reason this witness can't submit in this env (e.g. missing key).
    fn available(&self) -> Result<(), String> {
        Ok(())
    }

    /// Fire-and-forget best-effort. Ok carries a note (job id, mirror used).
    /// The response is NEVER trusted for confirmation — only `newest` is.
    fn submit(&self, url: &str) -> Result<String, String>;

    /// Newest known snapshot via public read APIs only.
    fn newest(&self, url: &str) -> Result<Option<Snapshot>, String>;
}

/// Browser-ish UA: archive.today and friends captcha-wall obvious bots.
pub const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(UA)
        .redirects(8)
        .build()
}

pub(crate) fn short_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("http {code}"),
        other => other.to_string(),
    }
}

/// A submit error that's a transient per-IP cooldown (HTTP 429), not a real
/// failure — the caller should reconcile later rather than report broken.
/// Matches the "http {code}" shape `short_err` emits.
pub fn is_rate_limited(msg: &str) -> bool {
    msg.contains("429")
}

pub fn all() -> Vec<Box<dyn Witness>> {
    vec![
        Box::new(wayback::Wayback::from_env()),
        Box::new(archive_today::ArchiveToday::new()),
        Box::new(ghostarchive::GhostArchive::new()),
    ]
}

pub fn select(spec: &str) -> Result<Vec<Box<dyn Witness>>, String> {
    let mut out: Vec<Box<dyn Witness>> = Vec::new();
    for name in spec.split(',').map(|s| s.trim().to_ascii_lowercase()) {
        match name.as_str() {
            "" => continue,
            "wayback" | "wb" | "webarchive" => out.push(Box::new(wayback::Wayback::from_env())),
            "archivetoday" | "archive.today" | "at" => {
                out.push(Box::new(archive_today::ArchiveToday::new()))
            }
            "ghostarchive" | "ghost" | "ga" => {
                out.push(Box::new(ghostarchive::GhostArchive::new()))
            }
            other => {
                return Err(format!(
                    "unknown witness: {other} (valid: wayback, archivetoday, ghostarchive)"
                ));
            }
        }
    }
    if out.is_empty() {
        return Err("empty --witness list".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_with(note: Option<&str>) -> Testimony {
        let mut t = Testimony::pending("w", "https://x");
        t.note = note.map(String::from);
        t
    }

    #[test]
    fn reason_codes_cover_the_failure_axes() {
        // submitted-ok-but-slow loses no detail → caller keeps polling
        assert_eq!(pending_with(None).reason(), "not_yet");
        assert_eq!(pending_with(Some("https://archive.ph: http 429")).reason(), "rate_limited");
        assert_eq!(pending_with(Some("http 503")).reason(), "server_error");
        assert_eq!(pending_with(Some("http 500")).reason(), "server_error");
        assert_eq!(pending_with(Some("Network Error: timed out reading response")).reason(), "timeout");
        assert_eq!(pending_with(Some("dns error: failed to lookup")).reason(), "network");
        assert_eq!(pending_with(Some("some weird thing")).reason(), "error");
    }

    #[test]
    fn reason_terminal_states() {
        assert_eq!(
            Testimony::confirmed("w", "https://x", Snapshot { url: "u".into(), ts: None, epoch: None }).reason(),
            "ok"
        );
        assert_eq!(Testimony::absent("w", "https://x").reason(), "absent");
        assert_eq!(
            Testimony::skipped("w", "https://x", "ARCHIVE_ORG_KEY not set — free key: ...".into()).reason(),
            "no_key"
        );
    }

    #[test]
    fn rate_limit_detection() {
        assert!(is_rate_limited("http 429"));
        assert!(is_rate_limited("https://archive.ph: http 429"));
        assert!(!is_rate_limited("http 503"));
    }
}
