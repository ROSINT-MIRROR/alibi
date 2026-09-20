// Argument parsing is a pure function over argv: no env, no output, no network.

use std::time::Duration;

use alibi::args::{Mode, Parsed, parse};

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}

fn run_of(parts: &[&str]) -> alibi::args::Options {
    match parse(&argv(parts)).expect("parse succeeds") {
        Parsed::Run(options) => options,
        other => panic!("expected a run, got {other:?}"),
    }
}

fn usage_of(parts: &[&str]) -> String {
    match parse(&argv(parts)).expect("parse succeeds") {
        Parsed::Usage(message) => message,
        other => panic!("expected a usage error, got {other:?}"),
    }
}

#[test]
fn a_bare_url_establishes() {
    let options = run_of(&["https://example.com"]);
    assert_eq!(options.mode, Mode::Establish);
    assert_eq!(options.url, "https://example.com");
    assert!(!options.json);
}

#[test]
fn check_is_read_only_mode() {
    assert_eq!(
        run_of(&["--check", "https://example.com"]).mode,
        Mode::Check
    );
}

#[test]
fn no_wait_is_submit_mode() {
    assert_eq!(
        run_of(&["--no-wait", "https://example.com"]).mode,
        Mode::Submit
    );
}

#[test]
fn mcp_needs_no_url() {
    assert_eq!(run_of(&["--mcp"]).mode, Mode::Mcp);
}

#[test]
fn timeout_accepts_both_spellings() {
    let spaced = run_of(&["--timeout", "42", "https://example.com"]);
    let inline = run_of(&["--timeout=42", "https://example.com"]);
    assert_eq!(spaced.timeout, Duration::from_secs(42));
    assert_eq!(inline.timeout, Duration::from_secs(42));
}

#[test]
fn witness_accepts_both_spellings() {
    let spaced = run_of(&["--witness", "wayback", "https://example.com"]);
    let inline = run_of(&["--witness=wayback", "https://example.com"]);
    assert_eq!(spaced.witness_spec, "wayback");
    assert_eq!(inline.witness_spec, "wayback");
}

#[test]
fn help_and_version_short_circuit() {
    assert_eq!(parse(&argv(&["--help"])).expect("parses"), Parsed::Help);
    assert_eq!(parse(&argv(&["-h"])).expect("parses"), Parsed::Help);
    assert_eq!(
        parse(&argv(&["--version"])).expect("parses"),
        Parsed::Version
    );
    assert_eq!(parse(&argv(&["-V"])).expect("parses"), Parsed::Version);
}

#[test]
fn a_missing_url_is_a_usage_error() {
    assert_eq!(usage_of(&[]), "missing <url>");
}

#[test]
fn a_non_http_url_is_a_usage_error() {
    assert_eq!(
        usage_of(&["ftp://example.com"]),
        "url must start with http:// or https://"
    );
}

#[test]
fn two_urls_are_a_usage_error() {
    assert_eq!(
        usage_of(&["https://a.com", "https://b.com"]),
        "exactly one <url> expected"
    );
}

#[test]
fn check_and_no_wait_are_mutually_exclusive() {
    let message = usage_of(&["--check", "--no-wait", "https://example.com"]);
    assert_eq!(message, "--check and --no-wait are mutually exclusive");
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    assert_eq!(
        usage_of(&["--nope", "https://example.com"]),
        "unknown flag: --nope"
    );
}

#[test]
fn a_non_numeric_timeout_is_a_usage_error() {
    let message = usage_of(&["--timeout", "abc", "https://example.com"]);
    assert!(message.starts_with("--timeout needs seconds"));
}

#[test]
fn a_dangling_flag_value_is_a_usage_error() {
    assert_eq!(usage_of(&["--witness"]), "--witness needs a value");
    assert_eq!(usage_of(&["--timeout"]), "--timeout needs seconds");
}
