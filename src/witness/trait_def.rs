use crate::error::Error;
use crate::witness::record::Record;

pub enum Availability {
    Ready,
    Unusable(Error),
}

pub enum Attempted {
    Accepted(String),
    Refused(Error),
}

pub enum Reading {
    Found(Record),
    Failed(Error),
}

pub trait Witness: Send + Sync {
    fn name(&self) -> &'static str;

    fn available(&self) -> Result<Availability, Error> {
        Ok(Availability::Ready)
    }

    fn submit(&self, url: &str) -> Result<Attempted, Error>;

    fn newest(&self, url: &str) -> Result<Reading, Error>;
}
