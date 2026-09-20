use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use crate::config::{CONFIRM_SLACK, FIRST_POLL, FRESH_WINDOW, FUTURE_SLACK, POLL_EVERY};
use crate::error::Error;
use crate::pace::Pace;
use crate::reason::Reason;
use crate::time_util;
use crate::witness::record::Record;
use crate::witness::trait_def::{Attempted, Availability, Reading, Witness};
use crate::witness::types::{Snapshot, Status, Testimony};

pub trait EventSink {
    fn emit(&mut self, testimony: &Testimony) -> Result<(), Error>;
}

pub struct ClosureSink<F>(pub F);

impl<F: FnMut(&Testimony) -> Result<(), Error>> EventSink for ClosureSink<F> {
    fn emit(&mut self, testimony: &Testimony) -> Result<(), Error> {
        (self.0)(testimony)
    }
}

trait Job: Send + Sync {
    fn run(&self, witness: &dyn Witness, tx: &Sender<Testimony>) -> Result<(), Error>;
}

struct EstablishJob<'a> {
    url: &'a str,
    started: Instant,
    timeout: Duration,
    fresh_since: u64,
    confirm_since: u64,
    future_cap: u64,
    pace: &'a Pace,
}

struct CheckJob<'a> {
    url: &'a str,
}

struct SubmitJob<'a> {
    url: &'a str,
    pace: &'a Pace,
}

pub fn establish(
    url: &str,
    witnesses: &[Box<dyn Witness>],
    timeout: Duration,
    on_event: impl FnMut(&Testimony) -> Result<(), Error>,
) -> Result<Vec<Testimony>, Error> {
    let now = time_util::epoch_now()?;
    let pace = Pace::default_gap()?;
    let job = EstablishJob {
        url,
        started: Instant::now(),
        timeout,
        fresh_since: floor_sub(now, FRESH_WINDOW.as_secs()),
        confirm_since: floor_sub(now, CONFIRM_SLACK.as_secs()),
        future_cap: now + FUTURE_SLACK.as_secs(),
        pace: &pace,
    };
    run_fanout(witnesses, &mut ClosureSink(on_event), &job)
}

pub fn check(
    url: &str,
    witnesses: &[Box<dyn Witness>],
    on_event: impl FnMut(&Testimony) -> Result<(), Error>,
) -> Result<Vec<Testimony>, Error> {
    run_fanout(witnesses, &mut ClosureSink(on_event), &CheckJob { url })
}

pub fn submit_only(
    url: &str,
    witnesses: &[Box<dyn Witness>],
    on_event: impl FnMut(&Testimony) -> Result<(), Error>,
) -> Result<Vec<Testimony>, Error> {
    let pace = Pace::default_gap()?;
    run_fanout(
        witnesses,
        &mut ClosureSink(on_event),
        &SubmitJob { url, pace: &pace },
    )
}

impl Job for EstablishJob<'_> {
    fn run(&self, witness: &dyn Witness, tx: &Sender<Testimony>) -> Result<(), Error> {
        establish_one(witness, self, tx)
    }
}

impl Job for CheckJob<'_> {
    fn run(&self, witness: &dyn Witness, tx: &Sender<Testimony>) -> Result<(), Error> {
        let testimony = match witness.newest(self.url)? {
            Reading::Found(Record::Archived(snapshot)) => {
                confirmed(witness.name(), self.url, snapshot)
            }
            Reading::Found(Record::Nothing) => absent(witness.name(), self.url),
            Reading::Failed(failure) => errored(witness.name(), self.url, failure),
        };
        send(tx, testimony)
    }
}

impl Job for SubmitJob<'_> {
    fn run(&self, witness: &dyn Witness, tx: &Sender<Testimony>) -> Result<(), Error> {
        if let Availability::Unusable(unavailable) = witness.available()? {
            return send(tx, skipped(witness.name(), self.url, unavailable));
        }
        self.pace.wait_turn()?;
        let testimony = match witness.submit(self.url)? {
            Attempted::Accepted(note) => submitted(witness.name(), self.url, note),
            Attempted::Refused(failure) => submit_outcome(witness.name(), self.url, failure),
        };
        send(tx, testimony)
    }
}

enum Seen {
    Snapshot(Snapshot),
    Nothing,
}

enum LastFailure {
    Some(Error),
    None,
}

fn establish_one(
    witness: &dyn Witness,
    job: &EstablishJob,
    tx: &Sender<Testimony>,
) -> Result<(), Error> {
    if let Availability::Unusable(unavailable) = witness.available()? {
        return send(tx, skipped(witness.name(), job.url, unavailable));
    }
    let mut last = LastFailure::None;
    let pre = match witness.newest(job.url)? {
        Reading::Found(Record::Archived(snapshot)) => Seen::Snapshot(snapshot),
        Reading::Found(Record::Nothing) => Seen::Nothing,
        Reading::Failed(failure) => {
            last = LastFailure::Some(failure);
            Seen::Nothing
        }
    };
    if let Seen::Snapshot(snapshot) = &pre
        && fresh_enough(snapshot, job.fresh_since, job.future_cap)
    {
        send(
            tx,
            confirmed_fresh(witness.name(), job.url, snapshot.clone()),
        )?;
        return Ok(());
    }
    job.pace.wait_turn()?;
    match witness.submit(job.url)? {
        Attempted::Accepted(note) => send(tx, submitted(witness.name(), job.url, note))?,
        Attempted::Refused(failure) => {
            let note = submit_note(&failure);
            last = LastFailure::Some(failure);
            send(tx, submitted(witness.name(), job.url, note))?;
        }
    }
    poll_until_confirmed(witness, job, tx, &pre, last)
}

fn poll_until_confirmed(
    witness: &dyn Witness,
    job: &EstablishJob,
    tx: &Sender<Testimony>,
    pre: &Seen,
    mut last: LastFailure,
) -> Result<(), Error> {
    let mut wait = FIRST_POLL;
    loop {
        if job.started.elapsed() + wait > job.timeout {
            return send(tx, pending(witness.name(), job.url, last));
        }
        std::thread::sleep(wait);
        wait = POLL_EVERY;
        let current = match witness.newest(job.url)? {
            Reading::Found(Record::Archived(snapshot)) => snapshot,
            Reading::Found(Record::Nothing) => continue,
            Reading::Failed(failure) => {
                last = LastFailure::Some(failure);
                continue;
            }
        };
        if landed(&current, pre, job.confirm_since, job.future_cap) {
            return send(tx, confirmed(witness.name(), job.url, current));
        }
    }
}

fn fresh_enough(snapshot: &Snapshot, fresh_since: u64, future_cap: u64) -> bool {
    let Some(epoch) = snapshot.epoch else {
        return false;
    };
    epoch >= fresh_since && epoch <= future_cap
}

fn landed(current: &Snapshot, pre: &Seen, confirm_since: u64, future_cap: u64) -> bool {
    let Some(epoch) = current.epoch else {
        return is_new_id(current, pre);
    };
    epoch >= confirm_since && epoch <= future_cap
}

fn is_new_id(current: &Snapshot, pre: &Seen) -> bool {
    match pre {
        Seen::Snapshot(earlier) => earlier.url != current.url,
        Seen::Nothing => true,
    }
}

fn run_fanout(
    witnesses: &[Box<dyn Witness>],
    sink: &mut dyn EventSink,
    job: &dyn Job,
) -> Result<Vec<Testimony>, Error> {
    let mut terminal: Vec<Testimony> = Vec::new();
    std::thread::scope(|scope| -> Result<(), Error> {
        let (tx, rx) = mpsc::channel::<Testimony>();
        let mut handles = Vec::new();
        for witness in witnesses {
            let tx = tx.clone();
            handles.push(scope.spawn(move || job.run(witness.as_ref(), &tx)));
        }
        drop(tx);
        for testimony in rx {
            sink.emit(&testimony)?;
            if testimony.status != Status::Submitted {
                terminal.push(testimony);
            }
        }
        for handle in handles {
            match handle.join() {
                Ok(result) => result?,
                Err(panicked) => {
                    tracing::error!("witness thread panicked");
                    std::panic::resume_unwind(panicked)
                }
            }
        }
        Ok(())
    })?;
    terminal.sort_by_key(|testimony| order_of(witnesses, testimony));
    Ok(terminal)
}

fn order_of(witnesses: &[Box<dyn Witness>], testimony: &Testimony) -> usize {
    let mut index = 0;
    for witness in witnesses {
        if witness.name() == testimony.witness {
            return index;
        }
        index += 1;
    }
    index
}

fn floor_sub(now: u64, window: u64) -> u64 {
    let Some(earlier) = now.checked_sub(window) else {
        return 0;
    };
    earlier
}

fn send(tx: &Sender<Testimony>, testimony: Testimony) -> Result<(), Error> {
    tx.send(testimony).map_err(|_disconnected| Error::Channel)
}

fn submit_note(failure: &Error) -> String {
    if failure == &Error::Http(429) {
        format!("rate-limited ({failure}) — will end pending unless already public")
    } else {
        format!("submit failed ({failure}) — relying on poll")
    }
}

fn submit_outcome(witness: &'static str, url: &str, failure: Error) -> Testimony {
    if failure == Error::Http(429) {
        pending_rate(witness, url, failure)
    } else {
        errored(witness, url, failure)
    }
}

fn confirmed(witness: &'static str, url: &str, snapshot: Snapshot) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Confirmed,
        reason: Reason::Ok,
        snapshot: Some(snapshot),
        note: None,
    }
}

fn confirmed_fresh(witness: &'static str, url: &str, snapshot: Snapshot) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Confirmed,
        reason: Reason::Ok,
        snapshot: Some(snapshot),
        note: Some("fresh snapshot (<24h) already public — submit skipped".to_string()),
    }
}

fn pending(witness: &'static str, url: &str, last: LastFailure) -> Testimony {
    let LastFailure::Some(failure) = last else {
        return Testimony {
            witness,
            url: url.to_string(),
            status: Status::Pending,
            reason: Reason::NotYet,
            snapshot: None,
            note: None,
        };
    };
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Pending,
        reason: Reason::from(&failure),
        snapshot: None,
        note: Some(failure.to_string()),
    }
}

fn pending_rate(witness: &'static str, url: &str, failure: Error) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Pending,
        reason: Reason::from(&failure),
        snapshot: None,
        note: Some(format!("rate-limited ({failure}) — retry after cooldown")),
    }
}

fn absent(witness: &'static str, url: &str) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Absent,
        reason: Reason::Absent,
        snapshot: None,
        note: None,
    }
}

fn skipped(witness: &'static str, url: &str, failure: Error) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Skipped,
        reason: Reason::from(&failure),
        snapshot: None,
        note: Some(failure.to_string()),
    }
}

fn submitted(witness: &'static str, url: &str, note: String) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Submitted,
        reason: Reason::Submitted,
        snapshot: None,
        note: Some(note),
    }
}

fn errored(witness: &'static str, url: &str, failure: Error) -> Testimony {
    Testimony {
        witness,
        url: url.to_string(),
        status: Status::Error,
        reason: Reason::from(&failure),
        snapshot: None,
        note: Some(failure.to_string()),
    }
}
