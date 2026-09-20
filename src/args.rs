use std::time::Duration;

use crate::config::{DEFAULT_TIMEOUT, DEFAULT_WITNESSES};
use crate::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Establish,
    Check,
    Submit,
    Mcp,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
    pub json: bool,
    pub witness_spec: String,
    pub timeout: Duration,
    pub url: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Options),
    Help,
    Version,
    Usage(String),
}

struct Flags {
    check: bool,
    no_wait: bool,
    mcp: bool,
    json: bool,
    witness_spec: String,
    timeout: Duration,
    url: String,
}

pub fn parse(argv: &[String]) -> Result<Parsed, Error> {
    let mut flags = Flags {
        check: false,
        no_wait: false,
        mcp: false,
        json: false,
        witness_spec: DEFAULT_WITNESSES.to_string(),
        timeout: DEFAULT_TIMEOUT,
        url: String::new(),
    };
    let mut index = 0;
    while index < argv.len() {
        let arg = argv[index].as_str();
        index += 1;
        match arg {
            "--check" => flags.check = true,
            "--no-wait" => flags.no_wait = true,
            "--mcp" => flags.mcp = true,
            "--json" => flags.json = true,
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "--witness" => {
                let Some(value) = argv.get(index) else {
                    return Ok(Parsed::Usage("--witness needs a value".to_string()));
                };
                flags.witness_spec = value.clone();
                index += 1;
            }
            "--timeout" => {
                let Some(value) = argv.get(index) else {
                    return Ok(Parsed::Usage("--timeout needs seconds".to_string()));
                };
                match secs(value)? {
                    Seconds::Valid(duration) => flags.timeout = duration,
                    Seconds::Invalid(message) => return Ok(Parsed::Usage(message)),
                }
                index += 1;
            }
            other => match inline(other) {
                Inline::Witness(value) => flags.witness_spec = value,
                Inline::Timeout(value) => match secs(&value)? {
                    Seconds::Valid(duration) => flags.timeout = duration,
                    Seconds::Invalid(message) => return Ok(Parsed::Usage(message)),
                },
                Inline::Unknown(flag) => {
                    return Ok(Parsed::Usage(format!("unknown flag: {flag}")));
                }
                Inline::Positional(value) => {
                    if !flags.url.is_empty() {
                        return Ok(Parsed::Usage("exactly one <url> expected".to_string()));
                    }
                    flags.url = value;
                }
            },
        }
    }
    finish(flags)
}

enum Inline {
    Witness(String),
    Timeout(String),
    Unknown(String),
    Positional(String),
}

fn inline(arg: &str) -> Inline {
    let Some((flag, value)) = arg.split_once('=') else {
        if arg.starts_with('-') {
            return Inline::Unknown(arg.to_string());
        }
        return Inline::Positional(arg.to_string());
    };
    match flag {
        "--witness" => Inline::Witness(value.to_string()),
        "--timeout" => Inline::Timeout(value.to_string()),
        other => Inline::Unknown(other.to_string()),
    }
}

enum Seconds {
    Valid(Duration),
    Invalid(String),
}

fn secs(value: &str) -> Result<Seconds, Error> {
    match value.parse::<u64>() {
        Ok(parsed) => Ok(Seconds::Valid(Duration::from_secs(parsed))),
        Err(bad) => {
            tracing::warn!(error = %bad, "bad --timeout value, reporting usage error");
            Ok(Seconds::Invalid(format!("--timeout needs seconds: {bad}")))
        }
    }
}

fn finish(flags: Flags) -> Result<Parsed, Error> {
    if flags.mcp {
        return Ok(Parsed::Run(Options {
            mode: Mode::Mcp,
            json: flags.json,
            witness_spec: flags.witness_spec,
            timeout: flags.timeout,
            url: flags.url,
        }));
    }
    if flags.check && flags.no_wait {
        return Ok(Parsed::Usage(
            "--check and --no-wait are mutually exclusive".to_string(),
        ));
    }
    if flags.url.is_empty() {
        return Ok(Parsed::Usage("missing <url>".to_string()));
    }
    if !(flags.url.starts_with("http://") || flags.url.starts_with("https://")) {
        return Ok(Parsed::Usage(
            "url must start with http:// or https://".to_string(),
        ));
    }
    let mode = if flags.check {
        Mode::Check
    } else if flags.no_wait {
        Mode::Submit
    } else {
        Mode::Establish
    };
    Ok(Parsed::Run(Options {
        mode,
        json: flags.json,
        witness_spec: flags.witness_spec,
        timeout: flags.timeout,
        url: flags.url,
    }))
}
