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

// --- v0.112 structs --------------------------------------------------------

#[test]
fn structs_literals_fields_and_nesting() {
    // Struct literal, field access, by-value param, field assignment, and
    // nested structs with nested field access + nested field assignment.
    let src = r#"
const Point = struct { x: i32, y: i32 };
const Line = struct { a: Point, b: Point };

fn manhattan(p: Point) i32 { return p.x + p.y; }

pub fn main() i32 {
    var p: Point = Point{ .x = 3, .y = 4 };
    print(manhattan(p));
    p.x = 10;
    print(p.x);
    var l: Line = Line{ .a = Point{ .x = 1, .y = 2 }, .b = Point{ .x = 5, .y = 6 } };
    print(l.a.y);
    l.b.x = 99;
    print(l.b.x);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "7\n10\n2\n99\n");
}

#[test]
fn struct_returned_by_value() {
    let src = r#"
const Pair = struct { lo: i32, hi: i32 };
fn make(a: i32, b: i32) Pair { return Pair{ .lo = a, .hi = b }; }
pub fn main() i32 {
    var pr: Pair = make(11, 22);
    print(pr.lo + pr.hi);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "33\n");
}

// --- v0.113 struct methods + associated functions --------------------------

#[test]
fn struct_methods_assoc_and_chaining() {
    let src = r#"
const Counter = struct {
    n: i32,
    pub fn get(self: Counter) i32 { return self.n; }
    pub fn bumped(self: Counter, by: i32) Counter { return Counter{ .n = self.n + by }; }
    pub fn zero() Counter { return Counter{ .n = 0 }; }
};
pub fn main() i32 {
    var c: Counter = Counter.zero();   // associated fn
    print(c.get());                    // 0  (method)
    c = c.bumped(5);
    print(c.get());                    // 5
    var d: Counter = c.bumped(10).bumped(100);  // chained method calls
    print(d.get());                    // 115
    print(Counter.get(d));             // 115 (explicit-self static form)
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "0\n5\n115\n115\n");
}

// --- v0.114 optionals ------------------------------------------------------

#[test]
fn optionals_coercion_orelse_unwrap_and_field() {
    let src = r#"
const Box = struct { v: ?i32 };
fn maybe(b: bool) ?i32 {
    if (b) { return 42; }
    return null;
}
pub fn main() i32 {
    var x: ?i32 = 5;                 // T -> ?T coercion
    print(x orelse 0);               // 5
    var y: ?i32 = null;
    print(y orelse 99);              // 99
    print(maybe(true).?);            // 42
    print(maybe(false) orelse 7);    // 7
    var bx: Box = Box{ .v = 10 };    // field coercion
    print(bx.v orelse 0);            // 10
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "5\n99\n42\n7\n10\n");
}

#[test]
fn unwrapping_null_panics_with_exit_101() {
    let src = r#"
pub fn main() i32 {
    var z: ?i32 = null;
    print(z.?);
    return 0;
}
"#;
    let (code, _out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 101, "unwrapping null must panic with exit code 101");
}

// --- v0.115 error unions ---------------------------------------------------

#[test]
fn error_unions_try_propagation_and_catch() {
    let src = r#"
fn parseDigit(c: i32) !i32 {
    if (c < 48) { return error.TooLow; }
    if (c > 57) { return error.TooHigh; }
    return c - 48;
}
fn sumTwo(a: i32, b: i32) !i32 {
    var x: i32 = try parseDigit(a);   // try: propagate the error on failure
    var y: i32 = try parseDigit(b);
    return x + y;
}
pub fn main() i32 {
    print(sumTwo(53, 55) catch 0 - 1);   // 12  (5 + 7)
    print(sumTwo(50, 99) catch 0 - 1);   // -1  (error.TooHigh propagated)
    print(parseDigit(48) catch 100);     // 0
    print(parseDigit(40) catch 100);     // 100 (error.TooLow)
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "12\n-1\n0\n100\n");
}
