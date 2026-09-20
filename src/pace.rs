use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::SUBMIT_MIN_GAP;
use crate::error::Error;

pub struct Pace {
    next_free: Mutex<Instant>,
    gap: Duration,
}

impl Pace {
    pub fn new(gap: Duration) -> Result<Pace, Error> {
        Ok(Pace {
            next_free: Mutex::new(Instant::now()),
            gap,
        })
    }

    pub fn default_gap() -> Result<Pace, Error> {
        Pace::new(SUBMIT_MIN_GAP)
    }

    pub fn wait_turn(&self) -> Result<(), Error> {
        let sleep_for = self.reserve()?;
        std::thread::sleep(sleep_for);
        Ok(())
    }

    fn reserve(&self) -> Result<Duration, Error> {
        let mut next_free = match self.next_free.lock() {
            Ok(guard) => guard,
            Err(poisoned) => return Err(Error::Config(poisoned.to_string())),
        };
        let now = Instant::now();
        let ready_at = (*next_free).max(now);
        *next_free = ready_at + self.gap;
        Ok(ready_at.duration_since(now))
    }
}
