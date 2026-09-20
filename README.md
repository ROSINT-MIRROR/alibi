# alibi

Third-party witness archiver. `alibi <url>` gets a URL independent, publicly
verifiable alibis: snapshots on web.archive.org, archive.today and
ghostarchive.org.

Rust, one static binary, one job. Part of the `sherlockit` journalism kit
(sibling of `redstring`).

```
alibi https://example.com/article        # archive it, wait for confirmation
alibi --check https://example.com/       # read-only: what is already archived
```

## Why

Pages get scrubbed, paywalled and memory-holed. A local copy proves nothing —
you could have fabricated it. Independent third-party archives are the URL's
alibi, and the three default witnesses have **opposite failure modes**:

| Witness | Strength | Weakness |
|---|---|---|
| **wayback** (web.archive.org) | US nonprofit, court-citable, can issue affidavits | honors takedowns — pages *vanish* |
| **archive.today** | ignores robots.txt, never takes anything down, eats paywalls | anonymous operator, bus factor 1 |
| **ghostarchive.org** | third independent vote | small, no dates in its index |

One witness can fold or die. Three can't all be memory-holed at once.

## Install

```sh
cargo install --path .
```

`minimum rust 1.89` (edition 2024, let-chains). No system libraries: TLS is
rustls, so there is no OpenSSL to link against. The release binary is ~2.7 MB
and depends only on libc.

## Use

```
alibi [options] <url>            submit to all witnesses, poll until confirmed
alibi --check [options] <url>    read-only: list existing snapshots
alibi --no-wait [options] <url>  fire submits and exit; reconcile with --check
alibi --mcp                      serve MCP over stdio (JSON-RPC 2.0)

--witness LIST   comma list: wayback, archivetoday, ghostarchive (default: all)
--timeout SECS   establish deadline (default: 300)
--json           JSONL output instead of tab-separated lines
```

**stdout is data, stderr is noise.** One line per witness on stdout; progress and
warnings on stderr. Pipe-clean by construction.

```console
$ alibi --check https://example.com/
wayback	https://web.archive.org/web/20260101000000/https://example.com/
archivetoday	https://archive.ph/20260101000000/https://example.com/
ghostarchive	none
```

### Exit codes

`0` all requested witnesses confirmed · `1` some but not all · `2` none ·
`64` usage error · `70` internal error.

Made for shell chains: `alibi "$url" && echo "on the record"`.

### Machine output

`--json` emits one JSONL object per witness. The aggregate exit code is too
coarse for a caller deciding retry-now vs retry-later vs give-up, so the *cause*
rides on every line as a stable machine code. Never parse the prose.

```json
{"witness":"wayback","url":"https://example.com/","status":"confirmed","reason":"ok","snapshot":"https://web.archive.org/web/20260101000000/https://example.com/","ts":"20260101000000","detail":null}
```

`status`: `confirmed` · `pending` · `none` · `skipped` · `submitted` · `error`.

**Every status is data and every status reaches stdout** — in tab mode, in
`--json`, and over MCP alike. Nothing is silently dropped, so a caller that sees
no line for a witness knows the witness never ran, rather than having to guess
whether it was skipped. `skipped` carries the reason it was skipped (`no_key`),
and `submitted` is the transitional line `establish` prints when a submit leaves
the process.

`establish` is therefore a **stream**: a witness that gets submitted prints its
`submitted` line first and its terminal line later, so stdout carries more than
one line per witness. `--check` never submits and stays one line per witness.
Read the last line per witness for the verdict; the exit code already aggregates
the terminal states for you.

```console
$ alibi --json https://example.com/
{"witness":"wayback","url":"https://example.com/","status":"submitted","reason":"submitted",...}
{"witness":"wayback","url":"https://example.com/","status":"confirmed","reason":"ok",...}
```

`ts` is **always ts14** — `YYYYMMDDhhmmss`, UTC, zero-padded, exactly 14 digits —
for every witness, or `null`. Witnesses do not agree on a native format
(wayback's CDX speaks ts14, archive.today's timemap speaks RFC 1123), so alibi
converts at the parser and the wire never carries a witness's dialect. `null`
means the witness publishes no date at all: ghostarchive has no dates in its
index, so its `ts` is `null` even when `status` is `confirmed`. Never assume
`snapshot` and `ts` are both present.

| `reason` | meaning | what a caller should do |
|---|---|---|
| `ok` | confirmed in the public record | done |
| `absent` | the read says nothing is archived | archive it |
| `no_key` | wayback skipped, no `ARCHIVE_ORG_KEY` | set the key, or ignore |
| `rate_limited` | 429, transient per-IP cooldown | retry later, or via torsocks |
| `server_error` | 5xx | retry soon |
| `timeout` | slow read or connect | retry |
| `network` | DNS / connect / TLS | check connectivity |
| `not_yet` | submitted fine, not public yet at timeout | keep polling / `--check` later |
| `submitted` | a submit just left, verdict not in yet | wait for the witness's next line |
| `error` | anything else | read `detail` |

### The wayback key

Wayback submits exist **only** behind a key:

```sh
export ARCHIVE_ORG_KEY="accesskey:secret"   # free: https://archive.org/account/s3.php
```

Without it wayback is skipped with a note, it is not counted as a requested
witness, and the run proceeds on the keyless witnesses. `--check` never needs the
key — the read APIs are public.

The key is read **from the environment only** — never a CLI flag, which would
leak it into `ps` and shell history. It buys quota and priority, not attribution:
captures are not tied to an account anywhere.

## Doctrine

These are measured properties, not opinions. They are why the tool is shaped the
way it is.

**1. There is no reliable submit path.** Keyed SPN2 accepts fast but completes in
*minutes*. archive.today has cloudflare moods. So submits are fire-and-forget
best-effort: **nothing ever waits on a submit response or trusts its body.**

**2. The read side is reliable.** Confirmation comes only from public read APIs —
wayback CDX, archive.today timemap, ghostarchive search. Truth is "the public
record shows a snapshot fresher than our submit time", never "the submit said
ok".

**3. Pacing is the product, not politeness.** Bursting earns an IP a cooldown
that *creates* the 429s you are fighting. So: one submit per witness per URL per
run, no in-run retries, and a minimum gap between submits enforced by the pacer.
The next `alibi`/`--check` after the cooldown *is* the retry.

**4. A 429 is `pending`, not failure.** It means the capture didn't happen *this
attempt*. A re-run after the cooldown lands it, and the 24h freshness window
makes re-runs idempotent — a snapshot younger than a day is confirmed without
resubmitting.

**5. Mirrors share one backend.** archive.today's mirror domains front a single
limiter, so alibi rotates mirrors only on transport errors, never on an HTTP
refusal — rotating there would just re-hammer the same limiter.

**6. Troll mementos exist.** archive.today carries future-dated snapshots (there
is a 2035-dated one on example.com). Epochs are capped at now+2d or a lie would
shadow every real capture forever.

**7. It works under torsocks**, which gives a fresh-IP escape hatch when your own
address is in a cooldown.

## As an MCP server

`alibi --mcp` speaks JSON-RPC 2.0 over stdio and exposes two tools:

- **`alibi_check`** — read-only, submits nothing, needs no key. Safe to call
  repeatedly. This is the one an agent should reach for.
- **`alibi_establish`** — submits and polls. Slow and rate-limited; the tool
  description tells the model to call it once per URL and reconcile with
  `alibi_check` rather than retrying in a loop.

```json
{"mcpServers":{"alibi":{"command":"alibi","args":["--mcp"]}}}
```

The same pacer that guards the CLI guards the MCP path, so an agent that ignores
the advice still cannot burst.

## With Red String

Red String maintains the integration in its own `modules/alibi.ron`. It runs
`alibi --check --json <url>` as a subprocess, parses the JSONL and maps snapshot
URLs to graph entities and relations. Install `alibi` on `PATH` for Red String
to find it.

Alibi owns the CLI and JSONL output contract documented above. Module
configuration, output parsing and graph mapping belong to Red String; no module
specification or Red String dependency is bundled with Alibi.

It is wired read-only on purpose: archiving takes minutes and is rate-limited, so
a module firing on every node click would burn the journalist's IP into a
cooldown. Archive from the shell, then re-run the module to pick the snapshots
up.

## Scope

- **Websites only.** Video archiving is a separate tool.
- **No case or manifest awareness.** alibi is a pure function: URL in, witness
  URLs out. The case orchestrator records its stdout.
- **No local capture.** Frozen DOM, screenshots and WACZ are a different tool's
  job. alibi only procures third-party testimony.

## Privacy

Archiving a URL tells the archive operator that someone is interested in that
URL, and the request carries your IP. That includes archive.today, whose operator
is anonymous. This is inherent to public archiving, not a property of this tool —
but plan for it, and use torsocks if it matters.

alibi stores nothing locally, writes no config file and keeps no state between
runs. The archives are the database.

## Development

```sh
cargo build
cargo test          # 58 tests, all offline: every fixture is a recorded body
cargo clippy --all-targets
```

Two linters police this crate, and both must pass:

- **`build.rs`** — rejects erased error types, silent `.unwrap()`/`.ok()`,
  swallowed `Err`, and `pub fn` that don't return `Result`.
- **`lawkeeper`** — the stricter constitution: no comments in `src/`, no
  fabricated defaults outside the sanctum (`config.rs`), no recovery without an
  observed trace, tests only in `tests/`. It runs at **defaults, with no
  `law.toml` and no disabled rules.**

This is why every constant lives in `src/config.rs`, why all tests live in
`tests/`, and why the rationale that would normally be a code comment is in this
file instead.

### Dependencies

Four, each earning its place:

| Crate | Why |
|---|---|
| `ureq` | sync HTTP + rustls. Three threads don't need an async runtime. |
| `serde` / `serde_json` | the `--json` wire contract and CDX/SPN2 parsing |
| `tracing` | required by the law: a recovery that isn't observed is a banned silent failure. A facade only — with no subscriber installed it compiles to nothing, so the binary neither logs nor pays for it unless a host opts in. |

No async runtime, no clap (argv parsing is a hand-rolled indexed walk), no
config file.

## License

0BSD. See `LICENSE`.
