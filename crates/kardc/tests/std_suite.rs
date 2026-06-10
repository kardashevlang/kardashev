//! The in-language std test suites.
//!
//! Every `tests/std/*.ks` file is a kardashev program of `test` blocks that
//! `@import("std")` and exercise one std module (v0.154, Arc 5). Each file is
//! compiled through the full file-based pipeline in `EmitMode::Test` — exactly
//! what `kard test <file>` does — built at `-O0` (the v0.151 dev default for
//! tests) and run; its harness must report every test passing (exit code 0,
//! the failure count).
//!
//! Driving the corpus from one Rust test keeps `cargo test` the single
//! entry point for CI while the suites themselves stay in-language, where the
//! std code they pin lives.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kardc::backend::{BuildOptions, OptLevel};
use kardc::emit_c::EmitMode;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A process-unique temp path (mirrors the e2e harness's helper).
fn temp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("kardc_std_{}_{}_{}", tag, std::process::id(), n))
}

#[test]
fn std_suites_all_pass() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/std");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/std should exist at the repo root")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "ks").then_some(p)
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no std suites found in tests/std");

    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    for f in &files {
        let c = kardc::compile_program(f, EmitMode::Test).unwrap_or_else(|diags| {
            let src = std::fs::read_to_string(f).unwrap_or_default();
            panic!(
                "std suite {} failed to compile:\n{}",
                f.display(),
                kardc::diag::render_all(&diags, &f.display().to_string(), &src)
            )
        });
        let exe = temp_path("exe");
        kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build the test harness");
        let output = Command::new(&exe).output().expect("should run the harness");
        let _ = std::fs::remove_file(&exe);
        // The harness prints its ok/FAIL lines to STDERR (per-test status) and
        // any test-body `print` output to STDOUT — show both on failure.
        assert_eq!(
            output.status.code(),
            Some(0),
            "std suite {} had failing tests:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            f.display(),
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
