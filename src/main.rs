#![forbid(unsafe_code)]
#![deny(unused_must_use)]
#![deny(for_loops_over_fallibles)]
#![deny(dead_code)]
#![deny(unused_variables)]
#![deny(unused_assignments)]

use std::io::{self, Write};
use std::process::ExitCode;

use alibi::args::{Mode, Options, Parsed};
use alibi::config::{EXIT_ALL, EXIT_INTERNAL, EXIT_NONE, EXIT_SOME, EXIT_USAGE, HELP, KEY_ENV};
use alibi::mcp;
use alibi::render::{Line, note_of, write_line};
use alibi::witness::credential::{Credential, parse};
use alibi::{Error, Status, Testimony};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(failure) => {
            tracing::error!(error = %failure, "alibi failed");
            eprintln!("alibi: {failure}");
            ExitCode::from(EXIT_INTERNAL)
        }
    }
}

fn run() -> Result<ExitCode, Error> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let options = match alibi::args::parse(&argv)? {
        Parsed::Run(options) => options,
        Parsed::Help => {
            print!("{HELP}");
            return Ok(ExitCode::from(EXIT_ALL));
        }
        Parsed::Version => {
            println!("alibi {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::from(EXIT_ALL));
        }
        Parsed::Usage(message) => {
            eprintln!("alibi: {message}\n\n{HELP}");
            return Ok(ExitCode::from(EXIT_USAGE));
        }
    };
    let key = read_key()?;
    if let Mode::Mcp = options.mode {
        let stdin = io::stdin().lock();
        let mut stdout = io::stdout().lock();
        mcp::serve(stdin, &mut stdout, &key)?;
        return Ok(ExitCode::from(EXIT_ALL));
    }
    dispatch(&options, &key)
}

fn read_key() -> Result<Credential, Error> {
    match std::env::var(KEY_ENV) {
        Ok(raw) => parse(&raw),
        Err(unset) => {
            tracing::warn!(error = %unset, "ARCHIVE_ORG_KEY unset, wayback will skip");
            Ok(Credential::Absent)
        }
    }
}

fn dispatch(options: &Options, key: &Credential) -> Result<ExitCode, Error> {
    let witnesses = match alibi::select(&options.witness_spec, key) {
        Ok(list) => list,
        Err(bad) => {
            tracing::warn!(error = %bad, "bad --witness list");
            eprintln!("alibi: {bad}\n\n{HELP}");
            return Ok(ExitCode::from(EXIT_USAGE));
        }
    };
    let mut out = io::stdout().lock();
    let mut sink = |testimony: &Testimony| emit(&mut out, testimony, options.json);
    let results = match options.mode {
        Mode::Establish => alibi::establish(&options.url, &witnesses, options.timeout, &mut sink)?,
        Mode::Check => alibi::check(&options.url, &witnesses, &mut sink)?,
        Mode::Submit => alibi::submit_only(&options.url, &witnesses, &mut sink)?,
        Mode::Mcp => Vec::new(),
    };
    verdict(&options.mode, &results)
}

fn emit(out: &mut impl Write, testimony: &Testimony, json: bool) -> Result<(), Error> {
    if let Line::Data(note) = note_of(testimony)? {
        eprintln!("alibi: {note}");
    }
    write_line(out, testimony, json)
}

fn verdict(mode: &Mode, results: &[Testimony]) -> Result<ExitCode, Error> {
    let counted: Vec<&Testimony> = results
        .iter()
        .filter(|t| t.status != Status::Skipped)
        .collect();
    if counted.is_empty() {
        eprintln!("alibi: no usable witnesses (set ARCHIVE_ORG_KEY or widen --witness)");
        return Ok(ExitCode::from(EXIT_NONE));
    }
    if let Mode::Submit = mode {
        let fired = counted
            .iter()
            .filter(|t| t.status == Status::Submitted)
            .count();
        if fired > 0 {
            return Ok(ExitCode::from(EXIT_ALL));
        }
        return Ok(ExitCode::from(EXIT_NONE));
    }
    let confirmed = counted
        .iter()
        .filter(|t| t.status == Status::Confirmed)
        .count();
    if confirmed == counted.len() {
        return Ok(ExitCode::from(EXIT_ALL));
    }
    if confirmed > 0 {
        return Ok(ExitCode::from(EXIT_SOME));
    }
    Ok(ExitCode::from(EXIT_NONE))
}
