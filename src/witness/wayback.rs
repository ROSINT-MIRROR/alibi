use crate::config::{WAYBACK_CDX, WAYBACK_PREFIX, WAYBACK_SAVE};
use crate::error::Error;
use crate::time_util;
use crate::witness::credential::Credential;
use crate::witness::net::{agent, body_of};
use crate::witness::record::Record;
use crate::witness::trait_def::{Attempted, Availability, Reading, Witness};
use crate::witness::types::Snapshot;

pub struct Wayback {
    key: Credential,
}

impl Wayback {
    pub fn new(key: Credential) -> Result<Wayback, Error> {
        Ok(Wayback { key })
    }
}

impl Witness for Wayback {
    fn name(&self) -> &'static str {
        "wayback"
    }

    fn available(&self) -> Result<Availability, Error> {
        match &self.key {
            Credential::Key(_secret) => Ok(Availability::Ready),
            Credential::Absent => Ok(Availability::Unusable(Error::NoKey)),
        }
    }

    fn submit(&self, url: &str) -> Result<Attempted, Error> {
        let key = match &self.key {
            Credential::Key(secret) => secret,
            Credential::Absent => return Ok(Attempted::Refused(Error::NoKey)),
        };
        let sent = agent()?
            .post(WAYBACK_SAVE)
            .set("Accept", "application/json")
            .set("Authorization", &format!("LOW {key}"))
            .send_form(&[
                ("url", url),
                ("capture_all", "1"),
                ("skip_first_archive", "1"),
            ]);
        match sent {
            Ok(response) => Ok(Attempted::Accepted(job_note(&body_of(response)?)?)),
            Err(failure) => {
                let carried = Error::from(failure);
                tracing::warn!(error = %carried, "wayback submit refused, carrying to caller");
                Ok(Attempted::Refused(carried))
            }
        }
    }

    fn newest(&self, url: &str) -> Result<Reading, Error> {
        let request = agent()?
            .get(WAYBACK_CDX)
            .query("url", url)
            .query("output", "json")
            .query("limit", "-1")
            .query("fl", "timestamp,original");
        match request.call() {
            Ok(response) => Ok(Reading::Found(cdx_newest(&body_of(response)?)?)),
            Err(failure) => {
                let carried = Error::from(failure);
                tracing::warn!(error = %carried, "wayback cdx read failed, carrying to caller");
                Ok(Reading::Failed(carried))
            }
        }
    }
}

pub fn job_note(body: &str) -> Result<String, Error> {
    let accepted = "accepted".to_string();
    let parsed = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) => value,
        Err(malformed) => return Err(Error::Parse(malformed.to_string())),
    };
    let Some(job) = parsed.get("job_id").and_then(|id| id.as_str()) else {
        return Ok(accepted);
    };
    Ok(format!("job {job}"))
}

pub fn cdx_newest(body: &str) -> Result<Record, Error> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Record::Nothing);
    }
    let parsed = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => value,
        Err(malformed) => return Err(Error::Parse(malformed.to_string())),
    };
    let Some(rows) = parsed.as_array() else {
        return Err(Error::Parse("cdx body is not a json array".to_string()));
    };
    let Some(row) = rows.iter().skip(1).next_back() else {
        return Ok(Record::Nothing);
    };
    let Some(ts) = row.get(0).and_then(|cell| cell.as_str()) else {
        return Err(Error::Parse("cdx row has no timestamp".to_string()));
    };
    let Some(original) = row.get(1).and_then(|cell| cell.as_str()) else {
        return Err(Error::Parse("cdx row has no original url".to_string()));
    };
    let epoch = time_util::ts14_to_epoch(ts)?;
    Ok(Record::Archived(Snapshot {
        url: format!("{WAYBACK_PREFIX}{ts}/{original}"),
        epoch: Some(epoch),
        ts: Some(ts.to_string()),
    }))
}
