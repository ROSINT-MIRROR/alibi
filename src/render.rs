use std::io::Write;

use crate::error::Error;
use crate::witness::types::{Status, Testimony};

pub enum Line {
    Data(String),
    Silent,
}

fn line_for(testimony: &Testimony, json: bool) -> Result<String, Error> {
    if json {
        return json_line(testimony);
    }
    tab_line(testimony)
}

pub fn write_line(out: &mut impl Write, testimony: &Testimony, json: bool) -> Result<(), Error> {
    let text = line_for(testimony, json)?;
    writeln!(out, "{text}").map_err(|e| Error::Io(e.to_string()))?;
    out.flush().map_err(|e| Error::Io(e.to_string()))
}

pub fn note_of(testimony: &Testimony) -> Result<Line, Error> {
    let Some(note) = testimony.note.as_deref() else {
        return Ok(Line::Silent);
    };
    match testimony.status {
        Status::Submitted => Ok(Line::Data(format!("{}: {note}", testimony.witness))),
        Status::Skipped => Ok(Line::Data(format!(
            "{}: skipped — {note}",
            testimony.witness
        ))),
        Status::Pending | Status::Error => Ok(Line::Data(format!(
            "{}: {note} [{}]",
            testimony.witness, testimony.reason
        ))),
        Status::Confirmed | Status::Absent => Ok(Line::Silent),
    }
}

fn json_line(testimony: &Testimony) -> Result<String, Error> {
    let wire = testimony.wire()?;
    serde_json::to_string(&wire).map_err(|e| Error::Parse(e.to_string()))
}

fn tab_line(testimony: &Testimony) -> Result<String, Error> {
    let value = match testimony.status {
        Status::Confirmed => confirmed_value(testimony)?,
        Status::Pending => "pending".to_string(),
        Status::Absent => "none".to_string(),
        Status::Error => "error".to_string(),
        Status::Skipped => "skipped".to_string(),
        Status::Submitted => "submitted".to_string(),
    };
    Ok(format!("{}\t{}", testimony.witness, value))
}

fn confirmed_value(testimony: &Testimony) -> Result<String, Error> {
    let Some(snapshot) = &testimony.snapshot else {
        return Ok("confirmed".to_string());
    };
    Ok(snapshot.url.clone())
}
