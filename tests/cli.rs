// End-to-end tests against the built binary. These drive argv, env, exit codes
// and the stdout/stderr split. No case here reaches the network: every one is
// answered by argument validation or by the no-key skip before any request.

use std::io::Write;
use std::process::{Command, Stdio};

fn alibi() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_alibi"));
    command.env_remove("ARCHIVE_ORG_KEY");
    command
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Run {
    let output = alibi().args(args).output().expect("the binary runs");
    Run {
        code: output.status.code().expect("an exit code"),
        stdout: String::from_utf8(output.stdout).expect("utf-8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("utf-8 stderr"),
    }
}

#[test]
fn help_goes_to_stdout_and_exits_zero() {
    let result = run(&["--help"]);
    assert_eq!(result.code, 0);
    assert!(result.stdout.contains("third-party witness archiver"));
    assert!(result.stdout.contains("--check"));
    assert!(result.stderr.is_empty());
}

#[test]
fn version_prints_the_crate_version() {
    let result = run(&["--version"]);
    assert_eq!(result.code, 0);
    assert_eq!(
        result.stdout.trim(),
        format!("alibi {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn a_missing_url_exits_64_and_says_why_on_stderr() {
    let result = run(&[]);
    assert_eq!(result.code, 64);
    assert!(result.stderr.contains("missing <url>"));
    assert!(
        result.stdout.is_empty(),
        "usage errors must not pollute stdout"
    );
}

#[test]
fn a_non_http_url_exits_64() {
    let result = run(&["ftp://example.com"]);
    assert_eq!(result.code, 64);
    assert!(
        result
            .stderr
            .contains("url must start with http:// or https://")
    );
}

#[test]
fn an_unknown_witness_exits_64() {
    let result = run(&["--witness", "bogus", "https://example.com"]);
    assert_eq!(result.code, 64);
    assert!(result.stderr.contains("unknown witness: bogus"));
}

#[test]
fn mutually_exclusive_modes_exit_64() {
    let result = run(&["--check", "--no-wait", "https://example.com"]);
    assert_eq!(result.code, 64);
    assert!(result.stderr.contains("mutually exclusive"));
}

#[test]
fn wayback_without_a_key_is_skipped_and_submits_nothing() {
    // The only requested witness is skipped, so no usable witness remains:
    // exit 2, and not one network call was made. The skip is data, not noise:
    // it rides stdout like every other status, with the note on stderr.
    let result = run(&["--no-wait", "--witness", "wayback", "https://example.com"]);
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("ARCHIVE_ORG_KEY"));
    assert!(result.stderr.contains("no usable witnesses"));
    assert_eq!(result.stdout.trim(), "wayback\tskipped");
}

#[test]
fn a_skip_is_a_full_json_row_with_its_reason() {
    let result = run(&[
        "--no-wait",
        "--json",
        "--witness",
        "wayback",
        "https://example.com/",
    ]);
    assert_eq!(result.code, 2);
    let line = result.stdout.trim();
    let row: serde_json::Value = serde_json::from_str(line).expect("a json row");
    assert_eq!(row["witness"], "wayback");
    assert_eq!(row["url"], "https://example.com/");
    assert_eq!(row["status"], "skipped");
    assert_eq!(row["reason"], "no_key");
    assert!(row["snapshot"].is_null());
    assert!(row["ts"].is_null());
    assert!(
        row["detail"]
            .as_str()
            .expect("a detail string")
            .contains("ARCHIVE_ORG_KEY")
    );
}

#[test]
fn a_key_without_a_colon_is_treated_as_absent() {
    let output = alibi()
        .env("ARCHIVE_ORG_KEY", "notakey")
        .args(["--no-wait", "--witness", "wayback", "https://example.com"])
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code().expect("an exit code"), 2);
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(stderr.contains("ARCHIVE_ORG_KEY"));
}

#[test]
fn the_mcp_handshake_answers_on_stdout() {
    let mut child = alibi()
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    let mut stdin = child.stdin.take().expect("a stdin pipe");
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n")
        .expect("write initialize");
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n")
        .expect("write tools/list");
    drop(stdin);
    let output = child.wait_with_output().expect("the server exits on eof");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "one reply per request, got: {stdout}");
    assert!(lines[0].contains("\"protocolVersion\""));
    assert!(lines[1].contains("alibi_check"));
    assert!(lines[1].contains("alibi_establish"));
}
