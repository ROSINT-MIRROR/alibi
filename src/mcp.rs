use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::{DEFAULT_TIMEOUT, DEFAULT_WITNESSES, MCP_PROTOCOL, MCP_SERVER_NAME, VERSION};
use crate::error::Error;
use crate::orchestrate;
use crate::witness::credential::Credential;
use crate::witness::registry::select;
use crate::witness::types::Testimony;

pub enum Served {
    Reply(Value),
    Quiet,
}

pub fn serve(input: impl BufRead, out: &mut impl Write, key: &Credential) -> Result<(), Error> {
    for line in input.lines() {
        let text = match line {
            Ok(text) => text,
            Err(broken) => {
                tracing::warn!(error = %broken, "mcp stdin closed");
                return Ok(());
            }
        };
        if text.trim().is_empty() {
            continue;
        }
        let Served::Reply(reply) = handle_line(&text, key)? else {
            continue;
        };
        let rendered = serde_json::to_string(&reply).map_err(|e| Error::Parse(e.to_string()))?;
        writeln!(out, "{rendered}").map_err(|e| Error::Io(e.to_string()))?;
        out.flush().map_err(|e| Error::Io(e.to_string()))?;
    }
    Ok(())
}

pub fn handle_line(text: &str, key: &Credential) -> Result<Served, Error> {
    let request = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(malformed) => {
            tracing::warn!(error = %malformed, "mcp request not json");
            return Ok(Served::Reply(rpc_error(
                Value::Null,
                -32700,
                "parse error",
            )?));
        }
    };
    let Some(method) = request.get("method").and_then(|m| m.as_str()) else {
        return Ok(Served::Reply(rpc_error(Value::Null, -32600, "no method")?));
    };
    let Some(id) = request.get("id") else {
        return Ok(Served::Quiet);
    };
    dispatch(method, &request, id.clone(), key)
}

fn dispatch(method: &str, request: &Value, id: Value, key: &Credential) -> Result<Served, Error> {
    match method {
        "initialize" => Ok(Served::Reply(rpc_ok(id, initialize()?)?)),
        "tools/list" => Ok(Served::Reply(rpc_ok(id, tools()?)?)),
        "tools/call" => call_tool(request, id, key),
        other => {
            tracing::warn!(method = other, "unknown mcp method");
            Ok(Served::Reply(rpc_error(id, -32601, "method not found")?))
        }
    }
}

fn initialize() -> Result<Value, Error> {
    Ok(json!({
        "protocolVersion": MCP_PROTOCOL,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": MCP_SERVER_NAME, "version": VERSION },
    }))
}

fn tools() -> Result<Value, Error> {
    Ok(json!({
        "tools": [
            {
                "name": "alibi_check",
                "description":
                    "Read-only. List existing public archive snapshots for a URL. \
                     Submits nothing, needs no key, safe to call repeatedly.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "http(s) URL to look up" },
                        "witness": {
                            "type": "string",
                            "description": "comma list: wayback, archivetoday, ghostarchive",
                        },
                    },
                    "required": ["url"],
                },
            },
            {
                "name": "alibi_establish",
                "description":
                    "Submit a URL to public archives and poll until confirmed. \
                     Rate-limited and slow (minutes). Archives throttle per IP: \
                     call once per URL and reconcile later with alibi_check \
                     rather than retrying in a loop.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "http(s) URL to archive" },
                        "witness": {
                            "type": "string",
                            "description": "comma list: wayback, archivetoday, ghostarchive",
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "deadline in seconds (default 300)",
                        },
                    },
                    "required": ["url"],
                },
            },
        ]
    }))
}

struct Call {
    name: String,
    url: String,
    witness: String,
    timeout: Duration,
}

fn call_tool(request: &Value, id: Value, key: &Credential) -> Result<Served, Error> {
    let call = match read_call(request)? {
        Requested::Valid(call) => call,
        Requested::Rejected(message) => {
            return Ok(Served::Reply(rpc_error(id, -32602, &message)?));
        }
    };
    let witnesses = match select(&call.witness, key) {
        Ok(list) => list,
        Err(bad) => {
            tracing::warn!(error = %bad, "mcp bad witness list");
            return Ok(Served::Reply(rpc_error(id, -32602, &bad.to_string())?));
        }
    };
    let mut collected: Vec<Testimony> = Vec::new();
    let mut sink = |testimony: &Testimony| -> Result<(), Error> {
        collected.push(testimony.clone());
        Ok(())
    };
    match call.name.as_str() {
        "alibi_check" => orchestrate::check(&call.url, &witnesses, &mut sink)?,
        "alibi_establish" => {
            orchestrate::establish(&call.url, &witnesses, call.timeout, &mut sink)?
        }
        other => {
            tracing::warn!(tool = other, "unknown mcp tool");
            return Ok(Served::Reply(rpc_error(id, -32602, "unknown tool")?));
        }
    };
    Ok(Served::Reply(rpc_ok(id, tool_result(&collected)?)?))
}

enum Requested {
    Valid(Call),
    Rejected(String),
}

fn read_call(request: &Value) -> Result<Requested, Error> {
    let Some(params) = request.get("params") else {
        return Ok(Requested::Rejected("no params".to_string()));
    };
    let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
        return Ok(Requested::Rejected("no tool name".to_string()));
    };
    let Some(arguments) = params.get("arguments") else {
        return Ok(Requested::Rejected("no arguments".to_string()));
    };
    let Some(url) = arguments.get("url").and_then(|u| u.as_str()) else {
        return Ok(Requested::Rejected("url is required".to_string()));
    };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Ok(Requested::Rejected(
            "url must start with http:// or https://".to_string(),
        ));
    }
    Ok(Requested::Valid(Call {
        name: name.to_string(),
        url: url.to_string(),
        witness: witness_spec(arguments),
        timeout: timeout_of(arguments),
    }))
}

fn witness_spec(arguments: &Value) -> String {
    let Some(spec) = arguments.get("witness").and_then(|w| w.as_str()) else {
        return DEFAULT_WITNESSES.to_string();
    };
    spec.to_string()
}

fn timeout_of(arguments: &Value) -> Duration {
    let Some(secs) = arguments.get("timeout_secs").and_then(|t| t.as_u64()) else {
        return DEFAULT_TIMEOUT;
    };
    Duration::from_secs(secs)
}

fn tool_result(results: &[Testimony]) -> Result<Value, Error> {
    let mut lines: Vec<String> = Vec::new();
    for testimony in results {
        let wire = testimony.wire()?;
        let rendered = serde_json::to_string(&wire).map_err(|e| Error::Parse(e.to_string()))?;
        lines.push(rendered);
    }
    Ok(json!({
        "content": [{ "type": "text", "text": lines.join("\n") }],
        "isError": false,
    }))
}

fn rpc_ok(id: Value, result: Value) -> Result<Value, Error> {
    Ok(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Result<Value, Error> {
    Ok(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    }))
}
