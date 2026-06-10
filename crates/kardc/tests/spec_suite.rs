//! The SPEC conformance corpus (Arc 5, v0.155).
//!
//! Every `tests/spec/**/*.ks` file is a self-contained kardashev program that
//! pins one observable rule of SPEC.md, declared by comment **directives**:
//!
//! ```text
//! //SPEC: §11.2 a `T` value widens to `?T` at an init site
//! //EXIT: 0            (expected exit code; default 0)
//! //OUT: 42            (one expected stdout line; repeat in order — stdout
//!                       must equal exactly these lines, each '\n'-terminated;
//!                       no OUT lines = stdout must be empty)
//! //STDIN: hello       (one stdin line to feed; repeat in order)
//! //ERR: E0312         (the program must FAIL to compile and every listed
//!                       code must appear among the diagnostics; mutually
//!                       exclusive with EXIT/OUT/STDIN)
//! ```
//!
//! Files (or directories) whose name starts with `_` are import fixtures —
//! helper modules a test `@import`s — and are skipped by the walk.
//!
//! The runner compiles each file through the file-based pipeline (so
//! `@import("std")` works), builds at `-O0` (the v0.151 dev level — these are
//! correctness pins, not benchmarks), runs, and compares. Files run on a small
//! thread pool; every failure is reported with its path before the test
//! panics, so a corpus regression names all offenders at once.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use kardc::backend::{BuildOptions, OptLevel};
use kardc::emit_c::EmitMode;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("kardc_spec_{}_{}_{}", tag, std::process::id(), n))
}

/// One file's parsed directives.
struct Directives {
    spec: bool,
    exit: i32,
    out: Vec<String>,
    stdin: Vec<String>,
    errs: Vec<String>,
}

fn parse_directives(src: &str) -> Directives {
    let mut d = Directives {
        spec: false,
        exit: 0,
        out: Vec::new(),
        stdin: Vec::new(),
        errs: Vec::new(),
    };
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("//SPEC:") {
            d.spec = !rest.trim().is_empty();
        } else if let Some(rest) = t.strip_prefix("//EXIT:") {
            d.exit = rest.trim().parse().expect("//EXIT: takes an integer");
        } else if let Some(rest) = t.strip_prefix("//OUT:") {
            // One leading space after the colon is separator, the rest is data
            // (lets a test pin leading whitespace).
            d.out.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else if let Some(rest) = t.strip_prefix("//STDIN:") {
            d.stdin.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else if let Some(rest) = t.strip_prefix("//ERR:") {
            d.errs.push(rest.trim().to_string());
        }
    }
    d
}

/// Run one corpus file; `Ok(())` or a failure description.
fn run_one(path: &Path) -> Result<(), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let d = parse_directives(&src);
    if !d.spec {
        return Err("missing //SPEC: directive".into());
    }
    if !d.errs.is_empty() && (d.exit != 0 || !d.out.is_empty() || !d.stdin.is_empty()) {
        return Err("//ERR: is mutually exclusive with //EXIT://OUT://STDIN:".into());
    }

    let compiled = kardc::compile_program(path, EmitMode::Program);
    if !d.errs.is_empty() {
        return match compiled {
            Ok(_) => Err(format!("expected {:?} but the program compiled", d.errs)),
            Err(diags) => {
                let codes: Vec<&str> = diags.iter().map(|x| x.code).collect();
                for want in &d.errs {
                    if !codes.iter().any(|c| c == want) {
                        return Err(format!("expected {want}, got {codes:?}"));
                    }
                }
                Ok(())
            }
        };
    }

    let c = match compiled {
        Ok(c) => c,
        Err(diags) => {
            let codes: Vec<&str> = diags.iter().map(|x| x.code).collect();
            return Err(format!("failed to compile: {codes:?}"));
        }
    };
    let exe = temp_path("exe");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).map_err(|e| format!("cc failed: {e}"))?;

    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        for line in &d.stdin {
            let _ = writeln!(stdin, "{line}");
        }
        // stdin drops (closes) here — EOF for any extra @readLine.
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait failed: {e}"))?;
    let _ = std::fs::remove_file(&exe);

    let code = output.status.code().unwrap_or(-1);
    if code != d.exit {
        return Err(format!("exit {code}, expected {}", d.exit));
    }
    let got = String::from_utf8_lossy(&output.stdout);
    let want = if d.out.is_empty() {
        String::new()
    } else {
        d.out.join("\n") + "\n"
    };
    if got != want {
        return Err(format!("stdout mismatch:\n--- got ---\n{got}--- want ---\n{want}"));
    }
    Ok(())
}

fn collect_ks(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        // A leading underscore (file or directory) marks an import FIXTURE —
        // a helper module some test `@import`s — not a test program itself
        // (it has no directives and never runs standalone).
        let name = e.file_name();
        if name.to_string_lossy().starts_with('_') {
            continue;
        }
        if p.is_dir() {
            collect_ks(&p, into);
        } else if p.extension().is_some_and(|x| x == "ks") {
            into.push(p);
        }
    }
}

#[test]
fn spec_corpus_conforms() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/spec");
    let mut files = Vec::new();
    collect_ks(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no spec corpus found in tests/spec");

    // A small worker pool: each thread claims the next unclaimed index. cc
    // dominates each file's wall time, so threads scale near-linearly.
    let next = AtomicUsize::new(0);
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(f) = files.get(i) else { break };
                if let Err(msg) = run_one(f) {
                    failures
                        .lock()
                        .unwrap()
                        .push(format!("{}: {msg}", f.display()));
                }
            });
        }
    });

    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "{} of {} spec files failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
