// The output contract: every status is data. No status is silently dropped on
// stdout, in tab mode or in JSON, and the JSON row a caller branches on is the
// same row the MCP path serves. Pure rendering — no network, no process.

use alibi::render::write_line;
use alibi::{Reason, Snapshot, Status, Testimony};

fn testimony(status: Status, reason: Reason, snapshot: Option<Snapshot>) -> Testimony {
    Testimony {
        witness: "wayback",
        url: "https://example.com/".to_string(),
        status,
        reason,
        snapshot,
        note: None,
    }
}

fn rendered(testimony: &Testimony, json: bool) -> String {
    let mut out: Vec<u8> = Vec::new();
    write_line(&mut out, testimony, json).expect("the line writes");
    String::from_utf8(out).expect("utf-8 output")
}

fn every_status() -> Vec<(Status, Reason)> {
    vec![
        (Status::Confirmed, Reason::Ok),
        (Status::Pending, Reason::NotYet),
        (Status::Absent, Reason::Absent),
        (Status::Skipped, Reason::NoKey),
        (Status::Submitted, Reason::Submitted),
        (Status::Error, Reason::Network),
    ]
}

#[test]
fn no_status_is_silent_in_tab_mode() {
    for (status, reason) in every_status() {
        let line = rendered(&testimony(status, reason, None), false);
        assert!(
            line.starts_with("wayback\t"),
            "{status} produced no tab line: {line:?}"
        );
    }
}

#[test]
fn no_status_is_silent_in_json_mode() {
    for (status, reason) in every_status() {
        let line = rendered(&testimony(status, reason, None), true);
        let row: serde_json::Value = serde_json::from_str(line.trim()).expect("a json row");
        assert_eq!(
            row["status"],
            status.to_string(),
            "wrong status on {line:?}"
        );
        assert_eq!(
            row["reason"],
            reason.to_string(),
            "wrong reason on {line:?}"
        );
    }
}

#[test]
fn skipped_and_submitted_carry_their_machine_codes() {
    let skipped = rendered(&testimony(Status::Skipped, Reason::NoKey, None), true);
    let row: serde_json::Value = serde_json::from_str(skipped.trim()).expect("a json row");
    assert_eq!(row["status"], "skipped");
    assert_eq!(row["reason"], "no_key");

    let submitted = rendered(&testimony(Status::Submitted, Reason::Submitted, None), true);
    let row: serde_json::Value = serde_json::from_str(submitted.trim()).expect("a json row");
    assert_eq!(row["status"], "submitted");
    assert_eq!(row["reason"], "submitted");
}

#[test]
fn a_confirmed_tab_line_is_the_snapshot_url() {
    let snapshot = Snapshot {
        url: "https://web.archive.org/web/20260101000000/https://example.com/".to_string(),
        ts: Some("20260101000000".to_string()),
        epoch: Some(1_767_225_600),
    };
    let line = rendered(
        &testimony(Status::Confirmed, Reason::Ok, Some(snapshot)),
        false,
    );
    assert_eq!(
        line.trim(),
        "wayback\thttps://web.archive.org/web/20260101000000/https://example.com/"
    );
}

#[test]
fn the_json_row_always_carries_all_seven_keys() {
    for (status, reason) in every_status() {
        let line = rendered(&testimony(status, reason, None), true);
        let row: serde_json::Value = serde_json::from_str(line.trim()).expect("a json row");
        let object = row.as_object().expect("a json object");
        assert_eq!(object.len(), 7, "wrong key count on {line:?}");
        for key in [
            "witness", "url", "status", "reason", "snapshot", "ts", "detail",
        ] {
            assert!(object.contains_key(key), "{key} missing from {line:?}");
        }
    }
}
