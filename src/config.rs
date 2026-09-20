use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
pub const FRESH_WINDOW: Duration = Duration::from_secs(24 * 3600);
pub const FIRST_POLL: Duration = Duration::from_secs(5);
pub const POLL_EVERY: Duration = Duration::from_secs(15);
pub const CONFIRM_SLACK: Duration = Duration::from_secs(120);
pub const FUTURE_SLACK: Duration = Duration::from_secs(2 * 24 * 3600);

pub const SUBMIT_MIN_GAP: Duration = Duration::from_secs(5);
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
pub const HTTP_REDIRECTS: u32 = 8;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

pub const KEY_ENV: &str = "ARCHIVE_ORG_KEY";

pub const WAYBACK_SAVE: &str = "https://web.archive.org/save";
pub const WAYBACK_CDX: &str = "https://web.archive.org/cdx/search/cdx";
pub const WAYBACK_PREFIX: &str = "https://web.archive.org/web/";

pub const ARCHIVE_TODAY_MIRRORS: &[&str] = &[
    "https://archive.ph",
    "https://archive.today",
    "https://archive.is",
    "https://archive.md",
];

pub const GHOSTARCHIVE: &str = "https://ghostarchive.org";

pub const DEFAULT_WITNESSES: &str = "wayback,archivetoday,ghostarchive";

pub const S3_KEY_URL: &str = "https://archive.org/account/s3.php";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MCP_PROTOCOL: &str = "2024-11-05";
pub const MCP_SERVER_NAME: &str = "alibi";

pub const EXIT_ALL: u8 = 0;
pub const EXIT_SOME: u8 = 1;
pub const EXIT_NONE: u8 = 2;
pub const EXIT_USAGE: u8 = 64;
pub const EXIT_INTERNAL: u8 = 70;

pub const HELP: &str = "\
alibi — third-party witness archiver (sherlockit)

usage:
  alibi [options] <url>            submit to all witnesses, poll until confirmed
  alibi --check [options] <url>    read-only: list existing snapshots
  alibi --no-wait [options] <url>  fire submits and exit; reconcile with --check
  alibi --mcp                      serve MCP over stdio (JSON-RPC 2.0)

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
