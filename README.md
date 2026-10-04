<p align="center">
  <img src="docs/alibi-mark.svg" alt="Alibi logo" width="120">
</p>

<h1 align="center">Alibi</h1>

<p align="center">
  <a href="Cargo.toml"><img alt="version" src="https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2FROSINT-MIRROR%2Falibi%2Fmaster%2FCargo.toml&query=%24.package.version&label=version&color=informational"></a>
  <a href="LICENSE"><img alt="license: 0BSD" src="https://img.shields.io/badge/license-0BSD-blue"></a>
  <img alt="status: development preview" src="https://img.shields.io/badge/status-development%20preview-orange">
</p>

`alibi` is a command-line tool that gets a URL archived by independent public
web archives and confirms the snapshots from their public read APIs. A local copy
proves little; third-party snapshots are verifiable by anyone.

Witnesses: **web.archive.org** (Wayback), **archive.today**, **ghostarchive.org**.

> **Status:** development preview, single binary, Rust. Built and tested on Linux
> only; no releases. The CLI and JSONL contract below may still change.

## Install

Requires Rust 1.89+ (edition 2024). TLS is rustls; no OpenSSL or other system
libraries.

```sh
git clone https://github.com/ROSINT-MIRROR/alibi
cd alibi
cargo install --path . --locked
```

## Usage

```
alibi [options] <url>            submit to all witnesses, poll until confirmed
alibi --check [options] <url>    read-only: list existing snapshots, submits nothing
alibi --no-wait [options] <url>  submit and exit; confirm later with --check
alibi --mcp                      MCP server over stdio (JSON-RPC 2.0)

--witness LIST   comma list: wayback, archivetoday, ghostarchive (default: all)
--timeout SECS   deadline for confirmation (default: 300)
--json           JSONL output instead of tab-separated lines
```

One line per witness on stdout; progress and warnings on stderr.

```console
$ alibi --check https://example.com/
wayback	https://web.archive.org/web/20260101000000/https://example.com/
archivetoday	https://archive.ph/20260101000000/https://example.com/
ghostarchive	none
```

Exit codes: `0` all requested witnesses confirmed, `1` some, `2` none,
`64` usage error, `70` internal error.

### JSONL

```json
{"witness":"wayback","url":"https://example.com/","status":"confirmed","reason":"ok","snapshot":"https://web.archive.org/web/20260101000000/https://example.com/","ts":"20260101000000","detail":null}
```

- `status`: `confirmed`, `pending`, `none`, `skipped`, `submitted`, `error`.
- `reason`: `ok`, `absent`, `no_key`, `rate_limited`, `server_error`, `timeout`,
  `network`, `not_yet`, `submitted`, `error`.
- `ts` is always 14-digit UTC (`YYYYMMDDhhmmss`) or `null` (ghostarchive publishes
  no dates).
- Archiving mode streams: a `submitted` line first, the verdict later. Read the
  last line per witness. `--check` prints exactly one line per witness.
- Every status is printed, including skips, so a missing line means the witness
  did not run.

### Wayback key

Wayback submissions require an archive.org S3 key in the environment (never a
flag):

```sh
export ARCHIVE_ORG_KEY="accesskey:secret"   # https://archive.org/account/s3.php
```

Without it, Wayback is skipped (`reason: no_key`) and the run continues with the
other witnesses. `--check` needs no key.

## Behaviour

- Confirmation comes only from public read APIs (Wayback CDX, archive.today
  timemap, ghostarchive search), never from a submit response. A snapshot counts
  when it is newer than the submit time; one younger than 24 h is confirmed
  without resubmitting.
- One submit per witness per URL per run, no in-run retries, and at least 5 s
  between submits. HTTP 429 is reported as `pending`/`rate_limited`; re-running
  after the cooldown is the retry.
- archive.today mirrors are rotated only on transport errors, not on HTTP
  refusals (they share one rate limiter).
- When picking archive.today's newest snapshot, mementos dated more than 2 days
  in the future are passed over (used only if nothing else exists).
- No config file, no local state; nothing is stored between runs.

## MCP server

`alibi --mcp` exposes two tools over stdio:

- `alibi_check` — read-only, no key needed.
- `alibi_establish` — submits and polls; its description tells agents to call it
  once per URL and reconcile with `alibi_check`. The same pacer limits it.

```json
{"mcpServers":{"alibi":{"command":"alibi","args":["--mcp"]}}}
```

## Red String

[Red String](https://github.com/ROSINT-MIRROR/redstring) calls `alibi --check --json`
from its `alibi` lookup module and, per URL source, offers *Check existing
archives* and *Archive now*. The integration lives in Red String; Alibi has no
dependency on it.

## Privacy

Submitting or checking a URL tells the archive operators that someone is
interested in it and exposes your IP address to them, including archive.today,
whose operator is anonymous. Use `torsocks` or another network path if that
matters.

## Limitations

- Development preview; no releases, Linux only.
- Submissions are best effort: archive.today can refuse or rate-limit, Wayback
  submissions need a key and can take minutes to become visible, and ghostarchive
  snapshots carry no date.
- Websites only: no video archiving, no local capture (DOM, screenshots, WACZ).
- The test suite is offline (recorded fixtures and argument checks); behaviour
  against the live archives is not covered by automated tests.

## Development

```sh
cargo build
cargo test                    # 58 tests, offline
cargo clippy --all-targets
```

`build.rs` rejects erased error types, silent `.unwrap()`/`.ok()`, swallowed
errors and `pub fn` that do not return `Result`. Constants live in
`src/config.rs`; tests live in `tests/`. Dependencies: `ureq` (sync HTTP, rustls),
`serde`/`serde_json`, `tracing` (facade only, no subscriber).

## Licence

[0BSD](LICENSE).
