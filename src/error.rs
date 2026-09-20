use std::fmt;

use crate::config::S3_KEY_URL;
use crate::reason::Reason;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Http(u16),
    Timeout,
    Network(String),
    Parse(String),
    NoMirror,
    NoKey,
    Channel,
    Io(String),
    Clock(String),
    Config(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Http(code) => write!(f, "http {code}"),
            Error::Timeout => f.write_str("timed out"),
            Error::Network(detail) => write!(f, "network: {detail}"),
            Error::Parse(detail) => write!(f, "parse: {detail}"),
            Error::NoMirror => f.write_str("no mirror reachable"),
            Error::NoKey => write!(f, "ARCHIVE_ORG_KEY not set — free key: {S3_KEY_URL}"),
            Error::Channel => f.write_str("internal channel closed"),
            Error::Io(detail) => write!(f, "io: {detail}"),
            Error::Clock(detail) => write!(f, "clock: {detail}"),
            Error::Config(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(code, _response) => Error::Http(code),
            ureq::Error::Transport(transport) => from_transport(transport),
        }
    }
}

fn from_transport(transport: ureq::Transport) -> Error {
    let msg = transport.to_string();
    if msg.contains("timed out") || msg.contains("timeout") {
        Error::Timeout
    } else {
        Error::Network(msg)
    }
}

impl From<&Error> for Reason {
    fn from(e: &Error) -> Self {
        match e {
            Error::Http(429) => Reason::RateLimited,
            Error::Http(500 | 502 | 503 | 504) => Reason::ServerError,
            Error::Http(_code) => Reason::Error,
            Error::Timeout => Reason::Timeout,
            Error::Network(_detail) => Reason::Network,
            Error::Parse(_detail) => Reason::Error,
            Error::NoMirror => Reason::Network,
            Error::NoKey => Reason::NoKey,
            Error::Channel => Reason::Error,
            Error::Io(_detail) => Reason::Error,
            Error::Clock(_detail) => Reason::Error,
            Error::Config(_detail) => Reason::Error,
        }
    }
}
