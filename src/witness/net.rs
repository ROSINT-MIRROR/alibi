use crate::config::{HTTP_REDIRECTS, HTTP_TIMEOUT, USER_AGENT};
use crate::error::Error;

pub fn agent() -> Result<ureq::Agent, Error> {
    Ok(ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .redirects(HTTP_REDIRECTS)
        .build())
}

pub fn body_of(response: ureq::Response) -> Result<String, Error> {
    response
        .into_string()
        .map_err(|e| Error::Network(e.to_string()))
}
