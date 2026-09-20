#![forbid(unsafe_code)]
#![deny(unused_must_use)]
#![deny(for_loops_over_fallibles)]
#![deny(dead_code)]
#![deny(unused_variables)]
#![deny(unused_assignments)]

pub mod args;
pub mod attempt;
pub mod config;
pub mod error;
pub mod mcp;
pub mod orchestrate;
pub mod pace;
pub mod reason;
pub mod render;
pub mod time_util;
pub mod witness;

pub use error::Error;
pub use orchestrate::{check, establish, submit_only};
pub use reason::Reason;
pub use witness::record::Record;
pub use witness::registry::select;
pub use witness::trait_def::{Attempted, Availability, Reading, Witness};
pub use witness::types::{Snapshot, Status, Testimony, Wire};
