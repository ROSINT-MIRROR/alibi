use crate::config::GHOSTARCHIVE;
use crate::error::Error;
use crate::witness::net::{agent, body_of};
use crate::witness::record::Record;
use crate::witness::trait_def::{Attempted, Reading, Witness};
use crate::witness::types::Snapshot;

pub struct GhostArchive;

impl Witness for GhostArchive {
    fn name(&self) -> &'static str {
        "ghostarchive"
    }

    fn submit(&self, url: &str) -> Result<Attempted, Error> {
        let sent = agent()?
            .post(&format!("{GHOSTARCHIVE}/archive2"))
            .set("Referer", GHOSTARCHIVE)
            .send_form(&[("archive", url)]);
        match sent {
            Ok(_accepted) => Ok(Attempted::Accepted("accepted".to_string())),
            Err(failure) => {
                let carried = Error::from(failure);
                tracing::warn!(error = %carried, "ghostarchive submit refused, carrying to caller");
                Ok(Attempted::Refused(carried))
            }
        }
    }

    fn newest(&self, url: &str) -> Result<Reading, Error> {
        let request = agent()?
            .get(&format!("{GHOSTARCHIVE}/search"))
            .query("term", url);
        match request.call() {
            Ok(response) => Ok(Reading::Found(first_archive_link(&body_of(response)?)?)),
            Err(failure) => {
                let carried = Error::from(failure);
                tracing::warn!(error = %carried, "ghostarchive search failed, carrying to caller");
                Ok(Reading::Failed(carried))
            }
        }
    }
}

pub fn first_archive_link(html: &str) -> Result<Record, Error> {
    let Some(anchor) = html.find("href=\"/archive/") else {
        return Ok(Record::Nothing);
    };
    let rest = &html[anchor + 6..];
    let Some(end) = rest.find('"') else {
        return Ok(Record::Nothing);
    };
    let path = &rest[..end];
    Ok(Record::Archived(Snapshot {
        url: format!("{GHOSTARCHIVE}{path}"),
        ts: None,
        epoch: None,
    }))
}
