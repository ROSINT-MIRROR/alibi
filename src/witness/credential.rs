use crate::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential {
    Key(String),
    Absent,
}

pub fn parse(raw: &str) -> Result<Credential, Error> {
    let trimmed = raw.trim();
    if trimmed.contains(':') {
        Ok(Credential::Key(trimmed.to_string()))
    } else {
        Ok(Credential::Absent)
    }
}
