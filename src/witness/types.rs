use std::fmt;

use serde::Serialize;

use crate::error::Error;
use crate::reason::Reason;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub url: String,
    pub ts: Option<String>,
    pub epoch: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Confirmed,
    Pending,
    #[serde(rename = "none")]
    Absent,
    Skipped,
    Submitted,
    Error,
}

fn word(status: Status) -> &'static str {
    match status {
        Status::Confirmed => "confirmed",
        Status::Pending => "pending",
        Status::Absent => "none",
        Status::Skipped => "skipped",
        Status::Submitted => "submitted",
        Status::Error => "error",
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(word(*self))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Testimony {
    pub witness: &'static str,
    pub url: String,
    pub status: Status,
    pub reason: Reason,
    pub snapshot: Option<Snapshot>,
    pub note: Option<String>,
}

#[derive(Serialize)]
pub struct Wire<'a> {
    pub witness: &'a str,
    pub url: &'a str,
    pub status: Status,
    pub reason: Reason,
    pub snapshot: Option<&'a str>,
    pub ts: Option<&'a str>,
    pub detail: Option<&'a str>,
}

impl Testimony {
    pub fn wire(&self) -> Result<Wire<'_>, Error> {
        let snapshot = self.snapshot.as_ref();
        Ok(Wire {
            witness: self.witness,
            url: &self.url,
            status: self.status,
            reason: self.reason,
            snapshot: snapshot.map(|snap| snap.url.as_str()),
            ts: snapshot.and_then(|snap| snap.ts.as_deref()),
            detail: self.note.as_deref(),
        })
    }
}
