use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const MAX_LOC: usize = 800;

const SILENT_ERROR_PATTERNS: &[&str] = &[
    ".unwrap()",
    ".unwrap_or(",
    ".unwrap_or_default()",
    ".unwrap_or_else(",
    ".expect(",
    ".ok()",
    ".map_or(",
    ".map_or_else(",
    ".or(",
    ".or_else(",
    "let _ =",
    "_ =>",
    "if let Some(",
    "if let Ok(",
    "while let Some(",
    "while let Ok(",
];

const RESULT_OPTION_MATCH_ARMS: &[&str] = &[
    "None =>",
    "Some(_) =>",
    "Ok(_) =>",
    "Err(_) =>",
];

const ERR_DEFAULT_VALUES: &[&str] = &[
    "=> None",
    "=> 0",
    "=> false",
    "=> true",
    "=> \"\"",
    "=> String::new()",
    "=> Vec::new()",
    "=> HashMap::new()",
    "=> HashSet::new()",
    "=> BTreeMap::new()",
    "=> BTreeSet::new()",
    "=> Default::default()",
    "=> Ok(None)",
    "=> Ok(0)",
    "=> Ok(false)",
    "=> Ok(true)",
    "=> Ok(\"\")",
    "=> Ok(String::new())",
    "=> Ok(Vec::new())",
    "=> Ok(HashMap::new())",
    "=> Ok(Default::default())",
];

struct Hit {
    rule: &'static str,
    file: String,
    line: usize,
}

fn category_for(rule: &str) -> &'static str {
    if rule.starts_with("DECOMPOSITION:") {
        return "DECOMPOSITION";
    }
    if rule.starts_with("ERROR:") {
        return "ERROR";
    }
    if rule.starts_with("TYPE:") {
        return "TYPE";
    }
    if rule.starts_with("DEAD:") {
        return "DEAD";
    }
    "OTHER"
}

fn category_spirit(cat: &str) -> &'static str {
    match cat {
        "DECOMPOSITION" => "every file has one job. if it outgrew that job, split it.",
        "ERROR" => "propagate or crash. there is no third option. if an operation failed, the caller must know.",
        "TYPE" => "the type signature is the contract. if the error type is erased, the contract is worthless.",
        "DEAD" => "code that isn't serving the program right now is noise. noise misleads. delete it.",
        _ => "",
    }
}

fn rule_help(rule: &str) -> &'static str {
    match rule {
        "DECOMPOSITION:1" => "split by responsibility, not by arbitrary cut. if a file does two things, make two files.",
        "DECOMPOSITION:2" => "lib.rs and mod.rs are wiring — mod, use, attributes. move logic to its own file.",
        "ERROR:3" => concat!(
            ".unwrap(), .expect(), .ok(), .or(), if let Some/Ok, let _ =, _ => are all banned.\n",
            "    the ONLY legal moves: propagate with ? or return Err() explicitly.\n",
            "    there are exactly two honest responses to failure: tell the caller, or crash.",
        ),
        "ERROR:4" => concat!(
            "Ok(_), Err(_), Some(_), None => all throw away the value you're matching on.\n",
            "    bind it: Ok(val), Err(err), Some(val). if you don't need the value, you don't need the match.",
        ),
        "ERROR:5" => concat!(
            "returning Vec::new(), 0, None, false, or Default::default() from an error arm is lying.\n",
            "    the operation failed. say so. return Err and let the caller decide what 'empty' means.",
        ),
        "ERROR:6" => concat!(
            "an Err arm that does nothing is the #1 source of 'it silently stopped working' bugs.\n",
            "    you received an error, inspected it, and chose to pretend it didn't happen.",
        ),
        "TYPE:7" => concat!(
            "Result<T, ?> must carry a named, non-erased error type. erasure is banned:\n",
            "    String, &str, Box<dyn ...>, dyn ..., anyhow::Error, eyre::Report, (), bare primitives,\n",
            "    and lone single-letter generics. name your type — its identity IS the contract.",
        ),
        "DEAD:8" => concat!(
            "#[allow(dead_code/unused)] tapes over the check engine light.\n",
            "    the compiler says it's dead. trust it. delete the code — git has it if you ever need it back.",
        ),
        "DEAD:9" => concat!(
            "commented-out code rots, misleads, and makes grep lie to you.\n",
            "    version control exists. delete it. if you want it back: git log.",
        ),
        "DEAD:10" => concat!(
            "todo!(), unimplemented!(), unreachable!() are runtime panics wearing a trenchcoat.\n",
            "    either implement it now or remove the function. half-built code that compiles is worse than missing code.",
        ),
        "DEAD:11" => concat!(
            "/// and //! are library cosplay — this is an application, not an API shipped to strangers.\n",
            "    a doc comment restating the signature is noise. a constraint actually worth keeping\n",
            "    is a plain // line, and it answers to DEAD:9 like every other comment.",
        ),
        "TYPE:12" => concat!(
            "every pub fn — pub, pub(crate), pub(super), pub(in ...) — is an interface, and interfaces\n",
            "    report failure: return Result<T, YourError>, spelled out. no visibility qualifier,\n",
            "    const, async or unsafe dodges. internal wiring goes private; data goes in pub fields\n",
            "    and consts; trait impls implement someone else's contract and are the only exemption.",
        ),
        "TYPE:13" => concat!(
            "aliasing Result hides the error type from every signature and from this scanner.\n",
            "    the signature is the contract — write Result<T, YourError> at every fn, no shorthand.",
        ),
        _ => "",
    }
}

const CATEGORY_ORDER: &[&str] = &["DECOMPOSITION", "ERROR", "TYPE", "DEAD"];

fn format_report(hits: &[Hit]) -> String {
    let mut by_rule: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for hit in hits {
        by_rule
            .entry(hit.rule)
            .or_default()
            .push(format!("{}:{}", hit.file, hit.line));
    }

    let mut by_category: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for rule in by_rule.keys() {
        let cat = category_for(rule);
        let rules = by_category.entry(cat).or_default();
        if !rules.contains(rule) {
            rules.push(rule);
        }
    }

    let mut out = String::new();
    out.push_str(&format!("\n{} violations\n", hits.len()));

    for cat in CATEGORY_ORDER {
        let rules = match by_category.get(cat) {
            Some(r) => r,
            None => continue,
        };

        out.push_str(&format!(
            "\n--- {} --- {}\n",
            cat,
            category_spirit(cat)
        ));

        for rule in rules {
            let locations = match by_rule.get(rule) {
                Some(l) => l,
                None => continue,
            };

            out.push_str(&format!("\n  [{}] {}\n", rule, locations.join(" | ")));
            out.push_str(&format!("    {}\n", rule_help(rule)));
        }
    }

    out
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    // Watch the whole tree, not just the files found by this scan: cargo
    // recursively tracks a watched directory, so a file ADDED to src/ after
    // this run still triggers a re-scan. Per-file watches alone would let a
    // brand-new file dodge the law until its next edit.
    println!("cargo:rerun-if-changed={}", src_dir.display());
    let mut hits: Vec<Hit> = Vec::new();

    if src_dir.is_dir() {
        scan_dir(&manifest_dir, &src_dir, &mut hits);
    }

    if !hits.is_empty() {
        panic!("\n{}", format_report(&hits));
    }
}

fn hit(hits: &mut Vec<Hit>, rule: &'static str, file: &Path, line: usize) {
    hits.push(Hit {
        rule,
        file: file.to_string_lossy().to_string(),
        line,
    });
}

fn scan_dir(manifest_dir: &Path, dir: &Path, hits: &mut Vec<Hit>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(manifest_dir, &path, hits);
        } else if path.extension().map_or(false, |e| e == "rs") {
            scan_file(manifest_dir, &path, hits);
        }
    }
}

fn scan_file(manifest_dir: &Path, path: &Path, hits: &mut Vec<Hit>) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let src_dir = manifest_dir.join("src");
    let rel = path.strip_prefix(&src_dir).unwrap_or(path);
    let rel_str = rel.to_string_lossy();
    let file_name = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    let is_bin = rel_str.starts_with("bin/") || file_name == "main.rs";

    let loc = content.lines().count();
    if loc > MAX_LOC {
        hit(hits, "DECOMPOSITION:1", rel, 0);
    }

    let is_lib_or_mod = file_name == "lib.rs" || file_name == "mod.rs";
    if is_lib_or_mod {
        check_switchboard_purity(rel, &content, hits);
    }

    let mut in_block_comment = false;
    let mut in_test_module = false;
    let mut in_macro_rules = false;
    let mut macro_brace_depth: i32 = 0;

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.contains("/*") {
            in_block_comment = true;
        }
        if trimmed.contains("*/") {
            in_block_comment = false;
            continue;
        }
        if trimmed == "#[cfg(test)]" {
            in_test_module = true;
            continue;
        }

        if in_macro_rules {
            macro_brace_depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
            macro_brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
            if macro_brace_depth <= 0 {
                in_macro_rules = false;
            }
            continue;
        }
        if !in_block_comment
            && !trimmed.starts_with("//")
            && trimmed.starts_with("macro_rules!")
        {
            in_macro_rules = true;
            macro_brace_depth = trimmed.chars().filter(|&c| c == '{').count() as i32;
            macro_brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
            if macro_brace_depth <= 0 {
                in_macro_rules = false;
            }
            continue;
        }

        if in_block_comment || in_test_module {
            continue;
        }

        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            hit(hits, "DEAD:11", rel, line_no + 1);
            continue;
        }
        if trimmed.starts_with("//") {
            check_commented_out_code(rel, line_no, trimmed, hits);
            continue;
        }

        check_silent_errors(rel, line_no, trimmed, hits);
        check_wildcard_match_arms(rel, line_no, trimmed, hits);
        check_err_default_arms(rel, line_no, trimmed, hits);
        check_dead_code_suppression(rel, line_no, trimmed, hits);
        check_unfinished_markers(rel, line_no, trimmed, is_bin, hits);
        check_result_alias(rel, line_no, trimmed, hits);
    }

    check_empty_err_arms(rel, &content, hits);
    check_result_types(rel, &content, hits);
}

fn check_switchboard_purity(rel: &Path, content: &str, hits: &mut Vec<Hit>) {
    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_mod = trimmed.starts_with("mod ")
            || trimmed.starts_with("pub mod ")
            || trimmed.starts_with("pub(crate) mod ");
        let is_use = trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ")
            || trimmed.starts_with("pub(crate) use ");
        let is_attr = trimmed.starts_with("#[") || trimmed.starts_with("#![");
        let is_extern = trimmed.starts_with("extern crate ");
        if !is_mod && !is_use && !is_attr && !is_extern {
            hit(hits, "DECOMPOSITION:2", rel, line_no + 1);
        }
    }
}

fn check_silent_errors(rel: &Path, line_no: usize, trimmed: &str, hits: &mut Vec<Hit>) {
    for pattern in SILENT_ERROR_PATTERNS {
        if trimmed.contains(pattern) {
            hit(hits, "ERROR:3", rel, line_no + 1);
            break;
        }
    }
}

fn check_wildcard_match_arms(rel: &Path, line_no: usize, trimmed: &str, hits: &mut Vec<Hit>) {
    for pattern in RESULT_OPTION_MATCH_ARMS {
        if trimmed.starts_with(pattern) {
            hit(hits, "ERROR:4", rel, line_no + 1);
            break;
        }
    }
}

fn check_err_default_arms(rel: &Path, line_no: usize, trimmed: &str, hits: &mut Vec<Hit>) {
    if !(trimmed.starts_with("Err(")
        || trimmed.starts_with("Err (")
        || trimmed.starts_with("None =>"))
    {
        return;
    }
    if !trimmed.contains("=>") {
        return;
    }
    for pattern in ERR_DEFAULT_VALUES {
        if trimmed.contains(pattern) {
            hit(hits, "ERROR:5", rel, line_no + 1);
            break;
        }
    }
}

fn check_empty_err_arms(rel: &Path, content: &str, hits: &mut Vec<Hit>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_block_comment = false;
    let mut in_test_module = false;

    for idx in 0..lines.len() {
        let trimmed = lines[idx].trim();
        if trimmed.contains("/*") {
            in_block_comment = true;
        }
        if trimmed.contains("*/") {
            in_block_comment = false;
            continue;
        }
        if trimmed == "#[cfg(test)]" {
            in_test_module = true;
            continue;
        }
        if in_block_comment || in_test_module || trimmed.starts_with("//") {
            continue;
        }

        let is_err_arm =
            (trimmed.starts_with("Err(") || trimmed.starts_with("Err (")) && trimmed.contains("=>");
        if !is_err_arm {
            continue;
        }

        if trimmed.contains("=> {}") || trimmed.contains("=> { }") {
            hit(hits, "ERROR:6", rel, idx + 1);
            continue;
        }

        if trimmed.ends_with("=> {") || trimmed.ends_with("=>{") {
            let mut next_idx = idx + 1;
            while next_idx < lines.len() {
                let next_trimmed = lines[next_idx].trim();
                if next_trimmed.is_empty() || next_trimmed.starts_with("//") {
                    next_idx += 1;
                    continue;
                }
                if next_trimmed == "}" {
                    hit(hits, "ERROR:6", rel, idx + 1);
                }
                break;
            }
        }
    }
}

fn check_result_alias(rel: &Path, line_no: usize, trimmed: &str, hits: &mut Vec<Hit>) {
    let is_type_decl = trimmed.starts_with("type ")
        || trimmed.starts_with("pub type ")
        || trimmed.starts_with("pub(crate) type ")
        || trimmed.starts_with("pub(super) type ");
    if is_type_decl && trimmed.contains("Result<") {
        hit(hits, "TYPE:13", rel, line_no + 1);
    }
}

fn check_dead_code_suppression(rel: &Path, line_no: usize, trimmed: &str, hits: &mut Vec<Hit>) {
    if trimmed.contains("#[allow(dead_code")
        || trimmed.contains("#[allow(unused")
        || trimmed.contains("#[allow(unreachable_code")
    {
        hit(hits, "DEAD:8", rel, line_no + 1);
    }
}

fn check_commented_out_code(rel: &Path, line_no: usize, trimmed: &str, hits: &mut Vec<Hit>) {
    let after_slashes = trimmed.trim_start_matches('/').trim();

    if after_slashes.is_empty() {
        return;
    }

    let code_indicators = [
        "fn ", "let ", "use ", "pub ", "struct ", "enum ", "impl ", "mod ", "match ", "if ",
        "for ", "while ", "return ", "self.", "Self::", "crate::", "super::", ".await", "async ",
        "mut ", "const ", "static ", "type ", "trait ", "where ", "loop ", "break", "continue",
    ];

    let smells_like_code = code_indicators
        .iter()
        .any(|indicator| after_slashes.starts_with(indicator));

    let has_code_punctuation = after_slashes.ends_with(';')
        || after_slashes.ends_with('{')
        || after_slashes.ends_with('}')
        || after_slashes.ends_with("()")
        || (after_slashes.contains("::") && after_slashes.contains('('));

    if smells_like_code || has_code_punctuation {
        hit(hits, "DEAD:9", rel, line_no + 1);
    }
}

fn check_unfinished_markers(
    rel: &Path,
    line_no: usize,
    trimmed: &str,
    is_bin: bool,
    hits: &mut Vec<Hit>,
) {
    if is_bin && trimmed.contains("todo!") {
        return;
    }

    let markers = ["todo!(", "unimplemented!(", "unreachable!("];
    for marker in markers {
        if trimmed.contains(marker) {
            hit(hits, "DEAD:10", rel, line_no + 1);
            break;
        }
    }
}

fn check_result_types(rel: &Path, content: &str, hits: &mut Vec<Hit>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_block_comment = false;
    let mut in_test_module = false;
    let mut idx = 0usize;

    while idx < lines.len() {
        let trimmed = lines[idx].trim();

        if trimmed.contains("/*") {
            in_block_comment = true;
        }
        if trimmed.contains("*/") {
            in_block_comment = false;
            idx += 1;
            continue;
        }
        if trimmed == "#[cfg(test)]" {
            in_test_module = true;
            idx += 1;
            continue;
        }
        if in_block_comment || in_test_module || trimmed.starts_with("//") {
            idx += 1;
            continue;
        }

        if let Some(is_pub) = fn_visibility(trimmed) {
            let start_line = idx + 1;
            let mut sig = trimmed.to_string();
            let mut j = idx + 1;
            while j < lines.len()
                && !sig.contains('{')
                && !sig.trim_end().ends_with(';')
                && sig.len() < 1600
            {
                let next = lines[j].trim();
                if !next.starts_with("//") {
                    sig.push(' ');
                    sig.push_str(next);
                }
                j += 1;
            }
            check_return_type(rel, start_line, &sig, is_pub, hits);
            idx = j;
            continue;
        }

        idx += 1;
    }
}

// Recognizes a fn declaration through ANY prefix of visibility and fn
// qualifiers — pub, pub(crate), pub(super), pub(in path), const, async,
// unsafe, extern — so no qualifier combination dodges the scan.
// Returns Some(is_pub) for fn declarations, None otherwise.
fn fn_visibility(trimmed: &str) -> Option<bool> {
    let mut rest = trimmed;
    let mut is_pub = false;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("pub") {
            if after.starts_with('(') {
                let close = after.find(')')?;
                is_pub = true;
                rest = &after[close + 1..];
                continue;
            }
            if after.starts_with(' ') {
                is_pub = true;
                rest = after;
                continue;
            }
            return None;
        }
        let mut advanced = false;
        for q in ["const ", "async ", "unsafe ", "extern \"C\" ", "extern "] {
            if let Some(after) = rest.strip_prefix(q) {
                rest = after;
                advanced = true;
                break;
            }
        }
        if advanced {
            continue;
        }
        break;
    }
    if rest.starts_with("fn ") {
        return Some(is_pub);
    }
    None
}

const RESULT_RETURNS: &[&str] = &["Result<", "std::result::Result<"];

fn check_return_type(rel: &Path, line_no: usize, sig: &str, is_pub: bool, hits: &mut Vec<Hit>) {
    let normalized = sig.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.contains("fn main(") {
        return;
    }

    if is_pub {
        let returns_result = ret_of(sig)
            .map(|ret| RESULT_RETURNS.iter().any(|ok| ret.starts_with(ok)))
            .unwrap_or(false);
        if !returns_result {
            hit(hits, "TYPE:12", rel, line_no);
        }
    }

    let Some(arrow_idx) = sig.find("->") else {
        return;
    };

    let mut ret = sig[arrow_idx + 2..].trim();
    if let Some(w) = ret.find(" where ") {
        ret = ret[..w].trim();
    }
    if let Some(b) = ret.find('{') {
        ret = ret[..b].trim();
    }
    ret = ret.trim_end_matches(';').trim();

    if ret.starts_with("anyhow::Result") || ret.starts_with("eyre::Result") {
        hit(hits, "TYPE:7", rel, line_no);
        return;
    }

    if !ret.starts_with("Result<") && !ret.starts_with("std::result::Result<") {
        return;
    }

    let prefix_len = if ret.starts_with("std::result::Result<") {
        "std::result::Result<".len()
    } else {
        "Result<".len()
    };
    let Some(close_idx) = ret.rfind('>') else {
        hit(hits, "TYPE:7", rel, line_no);
        return;
    };

    let inner = &ret[prefix_len..close_idx];
    let Some(comma_idx) = find_top_level_comma(inner) else {
        hit(hits, "TYPE:7", rel, line_no);
        return;
    };

    let err_ty = inner[comma_idx + 1..].trim();
    if is_erased_err_type(err_ty) {
        hit(hits, "TYPE:7", rel, line_no);
    }
}

// The return type of a signature, with where clauses, bodies and trailing
// semicolons stripped. None when the fn returns ().
fn ret_of(sig: &str) -> Option<&str> {
    let arrow_idx = sig.find("->")?;
    let mut ret = sig[arrow_idx + 2..].trim();
    if let Some(w) = ret.find(" where ") {
        ret = ret[..w].trim();
    }
    if let Some(b) = ret.find('{') {
        ret = ret[..b].trim();
    }
    Some(ret.trim_end_matches(';').trim())
}

fn is_erased_err_type(err_ty: &str) -> bool {
    let normalized = err_ty.trim().trim_end_matches(',').trim();

    if normalized.is_empty() || normalized == "()" {
        return true;
    }

    const BANNED_EXACT: &[&str] = &[
        "String", "&str", "&'static str", "str",
        "anyhow::Error", "eyre::Report",
        "i8", "i16", "i32", "i64", "i128", "isize",
        "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char",
    ];
    if BANNED_EXACT.iter().any(|b| *b == normalized) {
        return true;
    }

    if normalized.contains("dyn ") {
        return true;
    }

    let chars: Vec<char> = normalized.chars().collect();
    if chars.len() == 1 && chars[0].is_ascii_uppercase() {
        return true;
    }

    false
}

fn find_top_level_comma(inner: &str) -> Option<usize> {
    let mut angle = 0usize;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;

    for (idx, ch) in inner.char_indices() {
        match ch {
            '<' => angle = angle.saturating_add(1),
            '>' => angle = angle.saturating_sub(1),
            '(' => paren = paren.saturating_add(1),
            ')' => paren = paren.saturating_sub(1),
            '[' => bracket = bracket.saturating_add(1),
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace = brace.saturating_add(1),
            '}' => brace = brace.saturating_sub(1),
            ',' if angle == 0 && paren == 0 && bracket == 0 && brace == 0 => return Some(idx),
            _ => {}
        }
    }

    None
}
