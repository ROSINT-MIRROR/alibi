//! alibi — third-party witness archiver (sherlockit kit).
//!
//! Stateless by doctrine: the archives ARE the database. Submits are
//! fire-and-forget best-effort; truth comes only from public read APIs.
//! redstring links this lib directly; main.rs is the thin CLI face.

#![forbid(unsafe_code)]

pub mod time_util;
pub mod witness;

pub use witness::{Snapshot, Status, Testimony, Witness, all, select};

use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// A snapshot younger than this makes re-runs idempotent: no resubmit.
pub const FRESH_WINDOW: Duration = Duration::from_secs(24 * 3600);
const FIRST_POLL: Duration = Duration::from_secs(5);
const POLL_EVERY: Duration = Duration::from_secs(15);
/// Captures start server-side before our submit returns; also absorbs clock skew.
const CONFIRM_SLACK: Duration = Duration::from_secs(120);
/// Mementos dated beyond this are lies (archive.today carries troll snapshots
/// dated e.g. 2035) — they must never satisfy a freshness test.
const FUTURE_SLACK: Duration = Duration::from_secs(2 * 24 * 3600);

/// Submit to every witness, poll the public record until each confirms or
/// `timeout` hits. `on_event` streams every event (Submitted/Skipped included);
/// the returned Vec holds one terminal Testimony per witness.
pub fn establish(
    url: &str,
    witnesses: &[Box<dyn Witness>],
    timeout: Duration,
    mut on_event: impl FnMut(&Testimony),
) -> Vec<Testimony> {
    let t0 = Instant::now();
    let now = time_util::epoch_now();
    let fresh_since = now.saturating_sub(FRESH_WINDOW.as_secs());
    let confirm_since = now.saturating_sub(CONFIRM_SLACK.as_secs());
    let future_cap = now + FUTURE_SLACK.as_secs();
    let credible = move |e: u64| e <= future_cap;

    run_fanout(witnesses, &mut on_event, move |w, tx| {
        if let Err(reason) = w.available() {
            let _ = tx.send(Testimony::skipped(w.name(), url, reason));
            return;
        }
        let pre = w.newest(url).ok().flatten();
        if let Some(p) = &pre
            && p.epoch.is_some_and(|e| e >= fresh_since && credible(e))
        {
            let mut t = Testimony::confirmed(w.name(), url, p.clone());
            t.note = Some("fresh snapshot (<24h) already public — submit skipped".into());
            let _ = tx.send(t);
            return;
        }
        // Last concrete failure seen (submit refusal or poll read-error), kept
        // so the terminal `pending` carries WHY — a 429/500/timeout the caller
        // can branch on, instead of a reasonless "pending".
        let mut last_detail: Option<String> = None;
        match w.submit(url) {
            Ok(note) => {
                let _ = tx.send(Testimony::submitted(w.name(), url, note));
            }
            Err(e) => {
                // not terminal: poll still decides. A 429 cooldown means this
                // attempt archived nothing → likely ends at `pending` (retry
                // later); other errors may still land server-side.
                let note = if witness::is_rate_limited(&e) {
                    format!("rate-limited ({e}) — will end pending unless already public")
                } else {
                    format!("submit failed ({e}) — relying on poll")
                };
                last_detail = Some(e);
                let _ = tx.send(Testimony::submitted(w.name(), url, note));
            }
        }
        let mut wait = FIRST_POLL;
        loop {
            if t0.elapsed() + wait > timeout {
                let mut t = Testimony::pending(w.name(), url);
                t.note = last_detail; // None = "submitted ok, just slow" (not_yet)
                let _ = tx.send(t);
                return;
            }
            std::thread::sleep(wait);
            wait = POLL_EVERY;
            let cur = match w.newest(url) {
                Ok(Some(snap)) => snap,
                Ok(None) => continue, // not landed yet — next tick
                Err(e) => {
                    last_detail = Some(e); // read hiccup (500/timeout) — remember why
                    continue;
                }
            };
            let fresh = match cur.epoch {
                Some(e) => e >= confirm_since && credible(e),
                // dateless witness (ghostarchive): any new snapshot id counts
                None => pre.as_ref().is_none_or(|p| p.url != cur.url),
            };
            if fresh {
                let _ = tx.send(Testimony::confirmed(w.name(), url, cur));
                return;
            }
        }
    })
}

/// Read-only: newest snapshot per witness, no submission. Never needs keys.
pub fn check(
    url: &str,
    witnesses: &[Box<dyn Witness>],
    mut on_event: impl FnMut(&Testimony),
) -> Vec<Testimony> {
    run_fanout(witnesses, &mut on_event, |w, tx| {
        let t = match w.newest(url) {
            Ok(Some(snap)) => Testimony::confirmed(w.name(), url, snap),
            Ok(None) => Testimony::absent(w.name(), url),
            Err(e) => Testimony::error(w.name(), url, e),
        };
        let _ = tx.send(t);
    })
}

/// Fire submits and return immediately; reconcile later with `check`.
pub fn submit_only(
    url: &str,
    witnesses: &[Box<dyn Witness>],
    mut on_event: impl FnMut(&Testimony),
) -> Vec<Testimony> {
    run_fanout(witnesses, &mut on_event, |w, tx| {
        if let Err(reason) = w.available() {
            let _ = tx.send(Testimony::skipped(w.name(), url, reason));
            return;
        }
        let t = match w.submit(url) {
            Ok(note) => Testimony::submitted(w.name(), url, note),
            // 429 = transient per-IP cooldown, not a failure: retry later / from
            // a clean IP. Pending (reconcilable) is honest; error implies broken.
            Err(e) if witness::is_rate_limited(&e) => {
                let mut t = Testimony::pending(w.name(), url);
                t.note = Some(format!("rate-limited ({e}) — retry after cooldown"));
                t
            }
            Err(e) => Testimony::error(w.name(), url, e),
        };
        let _ = tx.send(t);
    })
}

/// One thread per witness, events streamed to `on_event` as they happen,
/// terminal testimonies collected in witness order not arrival order.
fn run_fanout(
    witnesses: &[Box<dyn Witness>],
    on_event: &mut impl FnMut(&Testimony),
    job: impl Fn(&dyn Witness, &mpsc::Sender<Testimony>) + Send + Copy,
) -> Vec<Testimony> {
    let mut terminal = Vec::new();
    std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<Testimony>();
        for w in witnesses {
            let tx = tx.clone();
            s.spawn(move || job(w.as_ref(), &tx));
        }
        drop(tx);
        for t in rx {
            on_event(&t);
            if t.status.is_terminal() {
                terminal.push(t);
            }
        }
    });
    let order = |t: &Testimony| witnesses.iter().position(|w| w.name() == t.witness);
    terminal.sort_by_key(order);
    terminal
}
