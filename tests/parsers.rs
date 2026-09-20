// Parser tests drive the public API from tests/, per the law's TESTS:1.
// Every fixture here is a recorded response body: no network is touched.

use alibi::Record;
use alibi::time_util::{epoch_to_ts14, rfc1123_to_epoch, ts14_to_epoch};
use alibi::witness::archive_today::timemap_newest;
use alibi::witness::ghostarchive::first_archive_link;
use alibi::witness::wayback::{cdx_newest, job_note};

const TIMEMAP: &str = r#"<http://example.com/>; rel="original",
<https://archive.ph/timemap/http://example.com/>; rel="self"; type="application/link-format",
<https://archive.ph/20140712093046/http://example.com/>; rel="first memento"; datetime="Sat, 12 Jul 2014 09:30:46 GMT",
<https://archive.ph/20251225120000/http://example.com/>; rel="memento"; datetime="Thu, 25 Dec 2025 12:00:00 GMT",
<https://archive.ph/20260101000000/http://example.com/>; rel="last memento"; datetime="Thu, 01 Jan 2026 00:00:00 GMT"
"#;

fn archived_url(record: Record) -> String {
    match record {
        Record::Archived(snapshot) => snapshot.url,
        Record::Nothing => panic!("expected an archived snapshot, got nothing"),
    }
}

fn archived_ts(record: Record) -> Option<String> {
    match record {
        Record::Archived(snapshot) => snapshot.ts,
        Record::Nothing => panic!("expected an archived snapshot, got nothing"),
    }
}

#[test]
fn cdx_parses_the_newest_row() {
    let body = r#"[["timestamp","original"],
["20260526193415","https://www.ksl.com/article/51374190"],
["20260526193504","https://www.ksl.com/article/51374190"]]"#;
    let url = archived_url(cdx_newest(body).expect("cdx parses"));
    assert_eq!(
        url,
        "https://web.archive.org/web/20260526193504/https://www.ksl.com/article/51374190"
    );
}

#[test]
fn cdx_reports_nothing_for_an_empty_index() {
    assert_eq!(
        cdx_newest("[]").expect("empty array parses"),
        Record::Nothing
    );
    assert_eq!(cdx_newest("").expect("empty body parses"), Record::Nothing);
    let header_only = r#"[["timestamp","original"]]"#;
    assert_eq!(
        cdx_newest(header_only).expect("header only parses"),
        Record::Nothing
    );
}

#[test]
fn cdx_refuses_to_read_an_error_page_as_absence() {
    // A 503 HTML body is a read failure, never "nothing is archived".
    assert!(cdx_newest("<html>503</html>").is_err());
}

#[test]
fn job_note_reads_the_id_or_falls_back() {
    assert_eq!(
        job_note(r#"{"job_id":"abc-123"}"#).expect("json parses"),
        "job abc-123"
    );
    assert_eq!(
        job_note(r#"{"status":"ok"}"#).expect("json parses"),
        "accepted"
    );
}

#[test]
fn timemap_picks_the_freshest_memento() {
    let url = archived_url(timemap_newest(TIMEMAP, u64::MAX).expect("timemap parses"));
    assert_eq!(url, "https://archive.ph/20260101000000/http://example.com/");
}

#[test]
fn timemap_caps_future_dated_troll_mementos() {
    let troll = format!(
        "{TIMEMAP},\n<https://archive.ph/20351231145959/http://example.com/>; \
         rel=\"last memento\"; datetime=\"Mon, 31 Dec 2035 14:59:59 GMT\"\n"
    );
    let cap = ts14_to_epoch("20260701000000").expect("ts14 parses");
    let url = archived_url(timemap_newest(&troll, cap).expect("timemap parses"));
    assert_eq!(url, "https://archive.ph/20260101000000/http://example.com/");
}

#[test]
fn timemap_still_reports_when_every_memento_is_future_dated() {
    let only_troll = r#"<https://archive.ph/20351231145959/http://x/>; rel="memento"; datetime="Mon, 31 Dec 2035 14:59:59 GMT""#;
    let cap = ts14_to_epoch("20260701000000").expect("ts14 parses");
    let found = timemap_newest(only_troll, cap).expect("timemap parses");
    assert_ne!(found, Record::Nothing);
}

#[test]
fn timemap_reports_nothing_for_an_empty_body() {
    assert_eq!(
        timemap_newest("", u64::MAX).expect("empty parses"),
        Record::Nothing
    );
    assert_eq!(
        timemap_newest("<h1>404</h1>", u64::MAX).expect("html parses"),
        Record::Nothing
    );
}

#[test]
fn ghostarchive_finds_the_first_archive_href() {
    let html = r#"<a href="/search?term=x&amp;page=1">next</a>
<a href="/archive/2pWJE">hit</a><a href="/archive/5UOyX">hit2</a>"#;
    let url = archived_url(first_archive_link(html).expect("html parses"));
    assert_eq!(url, "https://ghostarchive.org/archive/2pWJE");
}

#[test]
fn ghostarchive_reports_nothing_when_there_are_no_results() {
    let found = first_archive_link("<html>nothing here</html>").expect("html parses");
    assert_eq!(found, Record::Nothing);
}

#[test]
fn ts14_round_trips_known_dates() {
    assert_eq!(ts14_to_epoch("19700101000000").expect("epoch zero"), 0);
    assert_eq!(
        ts14_to_epoch("20260610000000").expect("known date"),
        1_781_049_600
    );
}

#[test]
fn ts14_defaults_missing_parts_to_the_period_start() {
    let year_only = ts14_to_epoch("2026").expect("year parses");
    let full = ts14_to_epoch("20260101000000").expect("full parses");
    assert_eq!(year_only, full);
}

#[test]
fn ts14_rejects_garbage() {
    assert!(ts14_to_epoch("").is_err());
    assert!(ts14_to_epoch("not-a-ts").is_err());
    assert!(ts14_to_epoch("20269999").is_err());
}

#[test]
fn timemap_reports_ts14_not_the_rfc1123_it_read() {
    // The wire contract is ts14 for every witness; archive.today's timemap
    // speaks RFC1123, so the parser must convert rather than pass it through.
    let ts = archived_ts(timemap_newest(TIMEMAP, u64::MAX).expect("timemap parses"));
    assert_eq!(ts.as_deref(), Some("20260101000000"));
}

#[test]
fn epoch_to_ts14_round_trips_through_ts14_to_epoch() {
    for stamp in [
        "19700101000000",
        "20140712093046",
        "20260101000000",
        "20351231145959",
    ] {
        let epoch = ts14_to_epoch(stamp).expect("ts14 parses");
        assert_eq!(epoch_to_ts14(epoch).expect("epoch formats"), stamp);
    }
}

#[test]
fn epoch_to_ts14_matches_the_rfc1123_the_timemap_carries() {
    let epoch = rfc1123_to_epoch("Sat, 12 Jul 2014 09:30:46 GMT").expect("rfc1123 parses");
    assert_eq!(
        epoch_to_ts14(epoch).expect("epoch formats"),
        "20140712093046"
    );
}

#[test]
fn rfc1123_parses_with_and_without_the_weekday() {
    assert_eq!(
        rfc1123_to_epoch("Thu, 01 Jan 1970 00:00:00 GMT").expect("epoch zero"),
        0
    );
    assert_eq!(
        rfc1123_to_epoch("Wed, 10 Jun 2026 00:00:00 GMT").expect("known date"),
        1_781_049_600
    );
    assert_eq!(
        rfc1123_to_epoch("10 Jun 2026 00:00:00 GMT").expect("no weekday"),
        1_781_049_600
    );
    assert!(rfc1123_to_epoch("garbage").is_err());
}
