// MCP protocol surface: handshake, tool listing and argument rejection.
// Every case here is answered before any network call is attempted.

use std::time::Instant;

use alibi::mcp::{Served, handle_line};
use alibi::pace::Pace;
use alibi::witness::credential::{Credential, parse};
use serde_json::Value;

fn reply(line: &str) -> Value {
    match handle_line(line, &Credential::Absent).expect("handled") {
        Served::Reply(value) => value,
        Served::Quiet => panic!("expected a reply, got silence"),
    }
}

#[test]
fn initialize_returns_the_protocol_and_server_name() {
    let answer = reply(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    assert_eq!(answer["jsonrpc"], "2.0");
    assert_eq!(answer["id"], 1);
    assert_eq!(answer["result"]["serverInfo"]["name"], "alibi");
    assert!(answer["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_offers_check_and_establish() {
    let answer = reply(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let tools = answer["result"]["tools"].as_array().expect("a tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, vec!["alibi_check", "alibi_establish"]);
}

#[test]
fn the_establish_tool_warns_agents_not_to_loop() {
    let answer = reply(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#);
    let tools = answer["result"]["tools"].as_array().expect("a tools array");
    let establish = tools
        .iter()
        .find(|t| t["name"] == "alibi_establish")
        .expect("found");
    let description = establish["description"].as_str().expect("a description");
    assert!(description.contains("Rate-limited"));
    assert!(description.contains("rather than retrying in a loop"));
}

#[test]
fn a_notification_gets_no_reply() {
    let quiet = handle_line(
        r#"{"jsonrpc":"2.0","method":"initialized"}"#,
        &Credential::Absent,
    )
    .expect("handled");
    assert!(matches!(quiet, Served::Quiet));
}

#[test]
fn malformed_json_is_a_parse_error() {
    let answer = reply("{not json");
    assert_eq!(answer["error"]["code"], -32700);
}

#[test]
fn an_unknown_method_is_rejected() {
    let answer = reply(r#"{"jsonrpc":"2.0","id":4,"method":"nope"}"#);
    assert_eq!(answer["error"]["code"], -32601);
}

#[test]
fn a_missing_url_is_rejected_before_any_network_call() {
    let answer = reply(
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"alibi_check","arguments":{}}}"#,
    );
    assert_eq!(answer["error"]["code"], -32602);
    assert_eq!(answer["error"]["message"], "url is required");
}

#[test]
fn a_non_http_url_is_rejected_before_any_network_call() {
    let answer = reply(
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call",
            "params":{"name":"alibi_check","arguments":{"url":"file:///etc/passwd"}}}"#,
    );
    assert_eq!(answer["error"]["code"], -32602);
    assert_eq!(
        answer["error"]["message"],
        "url must start with http:// or https://"
    );
}

#[test]
fn an_unknown_witness_is_rejected_before_any_network_call() {
    let answer = reply(
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{"name":"alibi_check",
                      "arguments":{"url":"https://example.com","witness":"bogus"}}}"#,
    );
    assert_eq!(answer["error"]["code"], -32602);
    let message = answer["error"]["message"].as_str().expect("a message");
    assert!(message.contains("unknown witness: bogus"));
}

#[test]
fn the_credential_parser_rejects_a_key_without_a_colon() {
    assert_eq!(parse("notakey").expect("parses"), Credential::Absent);
    let good = parse("access:secret").expect("parses");
    assert_eq!(good, Credential::Key("access:secret".to_string()));
}

#[test]
fn the_pacer_serializes_submits_by_the_configured_gap() {
    let gap = std::time::Duration::from_millis(120);
    let pace = Pace::new(gap).expect("a pacer");
    let started = Instant::now();
    pace.wait_turn().expect("first turn");
    pace.wait_turn().expect("second turn");
    pace.wait_turn().expect("third turn");
    // The first turn is free; two gaps must have elapsed by the third.
    assert!(
        started.elapsed() >= gap * 2,
        "pacer did not space the submits"
    );
}

#[test]
fn the_pacer_lets_the_first_caller_through_immediately() {
    let pace = Pace::new(std::time::Duration::from_secs(30)).expect("a pacer");
    let started = Instant::now();
    pace.wait_turn().expect("first turn");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}
