use crate::attempt::Trail;
use crate::config::{ARCHIVE_TODAY_MIRRORS, FUTURE_SLACK};
use crate::error::Error;
use crate::time_util;
use crate::witness::net::{agent, body_of};
use crate::witness::record::Record;
use crate::witness::trait_def::{Attempted, Reading, Witness};
use crate::witness::types::Snapshot;

pub struct ArchiveToday;

impl Witness for ArchiveToday {
    fn name(&self) -> &'static str {
        "archivetoday"
    }

    fn submit(&self, url: &str) -> Result<Attempted, Error> {
        let mut trail = Trail::new()?;
        for mirror in ARCHIVE_TODAY_MIRRORS {
            let session = agent()?;
            let token = submitid(&session, mirror)?;
            let request = session
                .post(&format!("{mirror}/submit/"))
                .set("Referer", mirror);
            let mut form: Vec<(&str, &str)> = vec![("url", url)];
            if let Token::Found(id) = &token {
                form.push(("submitid", id.as_str()));
            }
            match request.send_form(&form) {
                Ok(_accepted) => return Ok(Attempted::Accepted(format!("via {mirror}"))),
                Err(refused @ ureq::Error::Status(..)) => {
                    let carried = Error::from(refused);
                    tracing::warn!(error = %carried, "archive.today refused, carrying to caller");
                    return Ok(Attempted::Refused(carried));
                }
                Err(transport) => {
                    let carried = Error::from(transport);
                    tracing::warn!(error = %carried, mirror, "mirror unreachable, rotating");
                    trail.record(mirror, carried)?;
                }
            }
        }
        Ok(Attempted::Refused(trail.into_error()?))
    }

    fn newest(&self, url: &str) -> Result<Reading, Error> {
        let cap = time_util::epoch_now()? + FUTURE_SLACK.as_secs();
        let mut trail = Trail::new()?;
        for mirror in ARCHIVE_TODAY_MIRRORS {
            match agent()?.get(&format!("{mirror}/timemap/{url}")).call() {
                Ok(response) => {
                    return Ok(Reading::Found(timemap_newest(&body_of(response)?, cap)?));
                }
                Err(other) => {
                    let carried = Error::from(other);
                    if carried == Error::Http(404) {
                        return Ok(Reading::Found(Record::Nothing));
                    }
                    tracing::warn!(error = %carried, mirror, "timemap read failed, rotating");
                    trail.record(mirror, carried)?;
                }
            }
        }
        Ok(Reading::Failed(trail.into_error()?))
    }
}

pub enum Token {
    Found(String),
    Missing,
}

fn submitid(session: &ureq::Agent, mirror: &str) -> Result<Token, Error> {
    match session.get(mirror).call() {
        Ok(response) => find_submitid(&body_of(response)?),
        Err(unreachable) => {
            let carried = Error::from(unreachable);
            tracing::warn!(error = %carried, mirror, "no submitid, posting without one");
            Ok(Token::Missing)
        }
    }
}

pub fn find_submitid(html: &str) -> Result<Token, Error> {
    let Some(anchor) = html.find("name=\"submitid\"") else {
        return Ok(Token::Missing);
    };
    let window = &html[anchor..(anchor + 400).min(html.len())];
    let Some(value_at) = window.find("value=\"") else {
        return Ok(Token::Missing);
    };
    let rest = &window[value_at + 7..];
    let Some(end) = rest.find('"') else {
        return Ok(Token::Missing);
    };
    let id = &rest[..end];
    if id.is_empty() {
        return Ok(Token::Missing);
    }
    Ok(Token::Found(id.to_string()))
}

struct Memento {
    url: String,
    epoch: u64,
}

enum Freshest {
    Seen(Memento),
    Empty,
}

impl Freshest {
    fn offer(self, candidate: Memento) -> Freshest {
        match self {
            Freshest::Seen(held) => {
                if candidate.epoch >= held.epoch {
                    Freshest::Seen(candidate)
                } else {
                    Freshest::Seen(held)
                }
            }
            Freshest::Empty => Freshest::Seen(candidate),
        }
    }
}

pub fn timemap_newest(body: &str, cap: u64) -> Result<Record, Error> {
    let mut capped = Freshest::Empty;
    let mut any = Freshest::Empty;
    for line in body.lines() {
        if !line.contains("memento") || !line.contains("datetime=\"") {
            continue;
        }
        let Some(open) = line.find('<') else { continue };
        let Some(close) = line[open + 1..].find('>') else {
            continue;
        };
        let url = &line[open + 1..open + 1 + close];
        let Some(stamp_at) = line.find("datetime=\"") else {
            continue;
        };
        let rest = &line[stamp_at + 10..];
        let Some(end) = rest.find('"') else { continue };
        let stamp = &rest[..end];
        let epoch = time_util::rfc1123_to_epoch(stamp)?;
        any = any.offer(Memento {
            url: url.to_string(),
            epoch,
        });
        if epoch <= cap {
            capped = capped.offer(Memento {
                url: url.to_string(),
                epoch,
            });
        }
    }
    let picked = if let Freshest::Seen(under_cap) = capped {
        Freshest::Seen(under_cap)
    } else {
        any
    };
    let Freshest::Seen(memento) = picked else {
        return Ok(Record::Nothing);
    };
    Ok(Record::Archived(Snapshot {
        url: memento.url,
        ts: Some(time_util::epoch_to_ts14(memento.epoch)?),
        epoch: Some(memento.epoch),
    }))
}
