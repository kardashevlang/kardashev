//! End-to-end tests.
//!
//! Each test compiles a real kardashev program to a native executable through
//! the C backend, runs it, and asserts its observable behaviour (exit code and
//! stdout). Together they exercise the whole pipeline:
//! `lex → parse → sema → emit C → cc → native binary → run`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kardc::emit_c::EmitMode;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A process-unique temp path (atomic counter survives the shared test PID).
fn temp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("kardc_e2e_{}_{}_{}", tag, std::process::id(), n))
}

/// Compile `src` in `mode` to a native executable, run it, and return its
/// `(exit_code, stdout)`.
fn build_and_capture(src: &str, mode: EmitMode) -> (i32, String) {
    let c = kardc::compile_to_c(src, mode).unwrap_or_else(|d| {
        panic!(
            "compile failed:\n{}",
            kardc::diag::render_all(&d, "test.ks", src)
        )
    });
    let exe = temp_path("exe");
    kardc::backend::cc_build(&c, &exe).expect("cc should build the emitted program");
    let output = Command::new(&exe).output().expect("should run the program");
    let _ = std::fs::remove_file(&exe);
    let code = output.status.code().unwrap_or(-1);
    (code, String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn hello_runs_with_defer_after_print() {
    // The defer must run at scope exit — *after* the value below is printed.
    let src = r#"
const LIMIT: i32 = comptime (5 * 2);
fn sum_to(n: i32) i32 {
    var total: i32 = 0;
    var i: i32 = 0;
    while (i < n) : (i = i + 1) { total = total + i; }
    return total;
}
pub fn main() i32 {
    defer print(999);
    print(sum_to(LIMIT));
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "45\n999\n");
}

#[test]
fn defer_lifo_across_loop_break_and_scope_exit() {
    // Pins the trickiest defer paths: a per-iteration defer that flushes on the
    // loop's fall-through *and* on `break`, plus LIFO ordering at function exit.
    let src = r#"
fn f(n: i32) i32 {
    var i: i32 = 0;
    while (i < n) : (i = i + 1) {
        defer print(100 + i);
        if (i == 2) { print(777); break; }
        print(i);
    }
    return 0;
}
pub fn main() i32 {
    defer print(1);
    defer print(2);
    var z: i32 = f(5);
    print(9);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "0\n100\n1\n101\n777\n102\n9\n2\n1\n");
}

#[test]
fn main_return_code_propagates() {
    let (code, _) = build_and_capture("pub fn main() i32 { return 42; }", EmitMode::Program);
    assert_eq!(code, 42);
}

#[test]
fn recursion_fib() {
    let src = r#"
fn fib(n: i32) i32 {
    if (n < 2) { return n; }
    return fib(n - 1) + fib(n - 2);
}
pub fn main() i32 { print(fib(10)); return 0; }
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "55\n");
}

#[test]
fn continue_runs_continue_clause() {
    // `continue` must run the `: (cont)` clause before looping. Summing only
    // even i in 0..6 via an explicit `continue` on odd values.
    let src = r#"
pub fn main() i32 {
    var sum: i32 = 0;
    var i: i32 = 0;
    while (i < 6) : (i = i + 1) {
        if (i % 2 == 1) { continue; }
        sum = sum + i;
    }
    print(sum);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "6\n"); // 0 + 2 + 4
}

#[test]
fn test_harness_exit_code_is_failure_count() {
    let src = r#"
fn id(x: i32) i32 { return x; }
test "passes" { expect(id(3) == 3); }
test "also passes" { expect(1 == 1); }
test "fails" { expect(id(3) == 4); }
"#;
    let (code, _) = build_and_capture(src, EmitMode::Test);
    assert_eq!(code, 1, "exactly one of three tests should fail");
}

#[test]
fn all_tests_pass_exit_zero() {
    let src = r#"
test "trivial" { expect(true); }
"#;
    let (code, _) = build_and_capture(src, EmitMode::Test);
    assert_eq!(code, 0);
}
