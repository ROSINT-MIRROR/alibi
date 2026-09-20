use serde::Serialize;

use crate::witness::types::Snapshot;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Record {
    Archived(Snapshot),
    Nothing,
}
