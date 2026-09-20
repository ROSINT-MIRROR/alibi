use std::fmt;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Ok,
    Absent,
    NoKey,
    RateLimited,
    ServerError,
    Timeout,
    Network,
    NotYet,
    Submitted,
    Error,
}

fn code(reason: Reason) -> &'static str {
    match reason {
        Reason::Ok => "ok",
        Reason::Absent => "absent",
        Reason::NoKey => "no_key",
        Reason::RateLimited => "rate_limited",
        Reason::ServerError => "server_error",
        Reason::Timeout => "timeout",
        Reason::Network => "network",
        Reason::NotYet => "not_yet",
        Reason::Submitted => "submitted",
        Reason::Error => "error",
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(code(*self))
    }
}
