use std::io::Write;
use std::time::Duration;

use alibi::{Status, Testimony};

const HELP: &str = "\
alibi — third-party witness archiver (sherlockit)

usage:
  alibi [options] <url>            submit to all witnesses, poll until confirmed
  alibi --check [options] <url>    read-only: list existing snapshots
  alibi --no-wait [options] <url>  fire submits and exit; reconcile with --check

options:
  --witness LIST   comma list of: wayback, archivetoday, ghostarchive (default: all)
  --timeout SECS   establish deadline (default: 300)
  --json           JSONL output instead of tab-separated lines
  -h, --help       this
  -V, --version    version

output: one line per witness on stdout: <witness>\\t<snapshot-url|pending|none|error>
        progress and warnings go to stderr; stdout stays pipe-clean
exit:   0 all witnesses confirmed · 1 some · 2 none · 64 usage error
env:    ARCHIVE_ORG_KEY=accesskey:secret enables the wayback witness
        (free: https://archive.org/account/s3.php); no key = wayback skipped
";

enum Mode {
    Establish,
    Check,
    Submit,
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut check = false;
    let mut no_wait = false;
    let mut json = false;
    let mut witness_spec: Option<String> = None;
    let mut timeout_secs: u64 = 300;
    let mut url: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--check" => check = true,
            "--no-wait" => no_wait = true,
            "--json" => json = true,
            "--witness" => match args.next() {
                Some(v) => witness_spec = Some(v),
                None => return usage("--witness needs a value"),
            },
            s if s.starts_with("--witness=") => {
                witness_spec = Some(s["--witness=".len()..].to_string());
            }
            "--timeout" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => timeout_secs = v,
                None => return usage("--timeout needs seconds"),
            },
            s if s.starts_with("--timeout=") => match s["--timeout=".len()..].parse() {
                Ok(v) => timeout_secs = v,
                Err(_) => return usage("--timeout needs seconds"),
            },
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "-V" | "--version" => {
                println!("alibi {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            s if s.starts_with('-') => return usage(&format!("unknown flag: {s}")),
            _ => {
                if url.is_some() {
                    return usage("exactly one <url> expected");
                }
                url = Some(a);
            }
        }
    }

    if check && no_wait {
        return usage("--check and --no-wait are mutually exclusive");
    }
    let Some(url) = url else {
        return usage("missing <url>");
    };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return usage("url must start with http:// or https://");
    }

    let witnesses = match witness_spec {
        Some(spec) => match alibi::select(&spec) {
            Ok(w) => w,
            Err(e) => return usage(&e),
        },
        None => alibi::all(),
    };

    let mode = if check {
        Mode::Check
    } else if no_wait {
        Mode::Submit
    } else {
        Mode::Establish
    };

    let mut out = std::io::stdout().lock();
    let mut emit = |t: &Testimony| {
        match t.status {
            Status::Submitted => {
                eprintln!("alibi: {}: {}", t.witness, t.note.as_deref().unwrap_or("submitted"));
            }
            Status::Skipped => {
                eprintln!("alibi: {}: skipped — {}", t.witness, t.note.as_deref().unwrap_or("?"));
            }
            Status::Confirmed | Status::Pending | Status::Absent | Status::Error => {
                match t.status {
                    // confirmed note is the fresh-skip message; print as-is
                    Status::Confirmed => {
                        if let Some(n) = t.note.as_deref() {
                            eprintln!("alibi: {}: {n}", t.witness);
                        }
                    }
                    // pending/error: surface WHY with the machine code in [brackets]
                    Status::Pending | Status::Error => {
                        let detail = t.note.as_deref().unwrap_or("not yet public");
                        eprintln!("alibi: {}: {detail} [{}]", t.witness, t.reason());
                    }
                    _ => {}
                }
                let line = if json {
                    serde_json::json!({
                        "witness": t.witness,
                        "url": t.url,
                        "status": t.status.as_str(),
                        "reason": t.reason(),
                        "snapshot": t.snapshot.as_ref().map(|s| s.url.as_str()),
                        "ts": t.snapshot.as_ref().and_then(|s| s.ts.as_deref()),
                        "detail": t.note.as_deref(),
                    })
                    .to_string()
                } else {
                    let val = match (&t.status, &t.snapshot) {
                        (Status::Confirmed, Some(s)) => s.url.as_str(),
                        (Status::Pending, _) => "pending",
                        (Status::Absent, _) => "none",
                        _ => "error",
                    };
                    format!("{}\t{}", t.witness, val)
                };
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        }
    };

    let results = match mode {
        Mode::Establish => {
            alibi::establish(&url, &witnesses, Duration::from_secs(timeout_secs), &mut emit)
        }
        Mode::Check => alibi::check(&url, &witnesses, &mut emit),
        Mode::Submit => alibi::submit_only(&url, &witnesses, &mut emit),
    };

    let counted: Vec<&Testimony> = results
        .iter()
        .filter(|t| t.status != Status::Skipped)
        .collect();
    if counted.is_empty() {
        eprintln!("alibi: no usable witnesses (set ARCHIVE_ORG_KEY or widen --witness)");
        return 2;
    }
    if matches!(mode, Mode::Submit) {
        let fired = counted.iter().any(|t| t.status == Status::Submitted);
        return if fired { 0 } else { 2 };
    }
    let confirmed = counted
        .iter()
        .filter(|t| t.status == Status::Confirmed)
        .count();
    if confirmed == counted.len() {
        0
    } else if confirmed > 0 {
        1
    } else {
        2
    }
}

fn usage(msg: &str) -> i32 {
    eprintln!("alibi: {msg}\n\n{HELP}");
    64
}
