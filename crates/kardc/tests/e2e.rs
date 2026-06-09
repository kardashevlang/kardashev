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
    kardc::backend::cc_build(&c, &exe, &kardc::backend::BuildOptions::default())
        .expect("cc should build the emitted program");
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

// --- v0.116 enums + switch -------------------------------------------------

#[test]
fn enums_and_exhaustive_switch() {
    let src = r#"
const Dir = enum { North, East, South, West };
fn turn(d: Dir) Dir {
    switch (d) {                  // exhaustive over all variants, no else
        .North => { return .East; },
        .East => { return .South; },
        .South => { return .West; },
        .West => { return .North; },
    }
}
fn code(d: Dir) i32 {
    switch (d) {
        .North => { return 0; },
        .East => { return 1; },
        .South => { return 2; },
        .West => { return 3; },
    }
}
fn bucket(n: i32) i32 {
    switch (n) {                  // integer switch with multi-label + else
        0 => { return 100; },
        1, 2 => { return 200; },
        else => { return 999; },
    }
}
pub fn main() i32 {
    print(code(turn(.North)));    // 1  (North -> East)
    print(code(turn(Dir.West)));  // 0  (West -> North), qualified literal
    print(bucket(0));             // 100
    print(bucket(2));             // 200
    print(bucket(7));             // 999
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "1\n0\n100\n200\n999\n");
}

// --- v0.117 fixed-size arrays ----------------------------------------------

#[test]
fn arrays_literal_index_assign_len_and_byvalue() {
    let src = r#"
fn sum(a: [4]i32) i32 {        // arrays pass by value
    var total: i32 = 0;
    var i: i32 = 0;
    while (i < 4) : (i = i + 1) {
        total = total + a[i];
    }
    return total;
}
pub fn main() i32 {
    var nums: [4]i32 = [4]i32{ 10, 20, 30, 40 };
    print(sum(nums));          // 100
    nums[1] = 99;
    print(nums[1]);            // 99
    print(nums.len);           // 4
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "100\n99\n4\n");
}

#[test]
fn array_index_out_of_bounds_panics_101() {
    let src = r#"
pub fn main() i32 {
    var a: [3]i32 = [3]i32{ 1, 2, 3 };
    var i: i32 = 5;
    print(a[i]);
    return 0;
}
"#;
    let (code, _out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 101, "out-of-bounds index must panic with exit 101");
}

// --- v0.118 pointers & slices ----------------------------------------------

#[test]
fn pointers_and_slices() {
    let src = r#"
fn bump(p: *i32) void { p.* = p.* + 1; }
fn sumSlice(s: []i32) i32 {
    var total: i32 = 0;
    var i: usize = 0;
    while (i < s.len) : (i = i + 1) { total = total + s[i]; }
    return total;
}
pub fn main() i32 {
    var x: i32 = 10;
    bump(&x);
    print(x);                 // 11  (mutation through *i32)
    var a: [5]i32 = [5]i32{ 1, 2, 3, 4, 5 };
    var s: []i32 = a[1..4];   // view of {2,3,4}
    print(s.len);             // 3
    print(sumSlice(s));       // 9
    s[0] = 100;               // writes through to the backing array
    print(a[1]);              // 100
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "11\n3\n9\n100\n");
}

#[test]
fn slice_index_out_of_bounds_panics_101() {
    let src = r#"
pub fn main() i32 {
    var a: [3]i32 = [3]i32{ 1, 2, 3 };
    var s: []i32 = a[0..2];
    var i: usize = 9;
    print(s[i]);
    return 0;
}
"#;
    let (code, _out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 101, "out-of-bounds slice index must panic with exit 101");
}

// --- v0.119 Allocator + heap -----------------------------------------------

#[test]
fn allocator_heap_alloc_write_and_free() {
    let src = r#"
fn sumSlice(s: []i32) i32 {
    var total: i32 = 0;
    var i: usize = 0;
    while (i < s.len) : (i = i + 1) { total = total + s[i]; }
    return total;
}
pub fn main() i32 {
    var a: Allocator = c_allocator();   // explicitly obtained + passed
    var xs: []i32 = alloc(a, i32, 5);   // heap-allocate a []i32 of length 5
    var i: usize = 0;
    while (i < xs.len) : (i = i + 1) { xs[i] = 10; }
    print(xs.len);          // 5
    print(sumSlice(xs));    // 50
    xs[2] = 99;
    print(sumSlice(xs));    // 139
    free(a, xs);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "5\n50\n139\n");
}

// --- v0.120 comptime generics ----------------------------------------------

#[test]
fn comptime_generics_monomorphised() {
    let src = r#"
fn max(comptime T: type, a: T, b: T) T {
    if (a > b) { return a; }
    return b;
}
fn max3(comptime T: type, a: T, b: T, c: T) T {
    return max(T, max(T, a, b), c);   // generic calling generic, forwarding T
}
fn id(comptime T: type, x: T) T { return x; }
pub fn main() i32 {
    print(max(i32, 3, 9));        // 9   (max instantiated at i32)
    print(max3(i32, 4, 11, 7));   // 11  (transitive i32 instantiation)
    print(id(i32, 42));           // 42
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "9\n11\n42\n");
}

// --- v0.121 type inference -------------------------------------------------

#[test]
fn type_inference_for_var_and_const() {
    let src = r#"
const MAX = 100;                          // inferred i64
const Point = struct { x: i32, y: i32 };
fn dist2(p: Point) i32 { return p.x * p.x + p.y * p.y; }
pub fn main() i32 {
    var n = 5;                            // inferred
    var sum = 0;
    var i = 0;
    while (i < n) : (i = i + 1) { sum = sum + i; }
    print(sum);                           // 10
    var p = Point{ .x = 3, .y = 4 };      // inferred struct
    print(dist2(p));                      // 25
    print(MAX);                           // 100
    var ok = true;                        // inferred bool
    if (ok) { print(1); }                 // 1
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "10\n25\n100\n1\n");
}

// --- v0.124 tagged unions + switch capture ---------------------------------

#[test]
fn tagged_unions_with_switch_capture() {
    let src = r#"
const Point = struct { x: i64, y: i64 };
const Shape = union(enum) {
    circle: i64,
    rect: Point,
};
fn area(s: Shape) i64 {
    switch (s) {
        .circle => |r| { return 3 * r * r; },   // capture the i64 payload
        .rect => |p| { return p.x * p.y; },      // capture the struct payload
    }
}
pub fn main() i32 {
    var c: Shape = Shape{ .circle = 10 };
    print(area(c));                              // 300
    var r: Shape = Shape{ .rect = Point{ .x = 4, .y = 5 } };
    print(area(r));                              // 20
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "300\n20\n");
}

// --- v0.125 optional if-capture + errdefer ---------------------------------

#[test]
fn if_capture_and_errdefer() {
    let src = r#"
fn lookup(found: bool) ?i32 {
    if (found) { return 42; }
    return null;
}
fn risky(bad: bool) !i32 {
    errdefer print(911);          // runs only on an error return
    if (bad) { return error.Boom; }
    return 7;
}
pub fn main() i32 {
    var a: ?i32 = lookup(true);
    if (a) |v| { print(v); } else { print(0); }     // 42
    var b: ?i32 = lookup(false);
    if (b) |v| { print(v); } else { print(99); }     // 99
    print(risky(false) catch 0 - 1);                  // 7   (errdefer did NOT fire)
    print(risky(true) catch 0 - 1);                   // 911 then -1 (errdefer fired)
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "42\n99\n7\n911\n-1\n");
}

// --- v0.127 strings --------------------------------------------------------

#[test]
fn strings_as_u8_slices() {
    let src = r#"
fn greet(name: []u8) void {
    print("Hello,");
    print(name);
}
pub fn main() i32 {
    greet("world");
    var s: []u8 = "kardashev";   // inferred []u8
    print(s);
    print(s.len);                // 9
    print(s[0]);                 // 107 ('k')
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "Hello,\nworld\nkardashev\n9\n107\n");
}

// --- v0.128 comptime value parameters --------------------------------------

#[test]
fn comptime_value_params_array_size_generics() {
    let src = r#"
fn dot(comptime n: usize, a: [n]i32, b: [n]i32) i32 {
    var total: i32 = 0;
    var i: usize = 0;
    while (i < n) : (i = i + 1) {     // n used as a comptime value
        total = total + a[i] * b[i];
    }
    return total;
}
pub fn main() i32 {
    print(dot(3, [3]i32{ 1, 2, 3 }, [3]i32{ 4, 5, 6 }));   // 32
    print(dot(2, [2]i32{ 10, 20 }, [2]i32{ 3, 4 }));        // 110 (distinct instantiation)
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "32\n110\n");
}

// --- v0.129 generic structs (type-returning functions) ---------------------

#[test]
fn generic_structs_via_type_constructor() {
    let src = r#"
fn Pair(comptime T: type) type {
    return struct { first: T, second: T };
}
const IntPair = Pair(i32);
const I64Pair = Pair(i64);

fn sum_pair(p: IntPair) i32 {
    return p.first + p.second;
}

pub fn main() i32 {
    var p: IntPair = IntPair{ .first = 10, .second = 32 };
    print(sum_pair(p));        // 42
    print(p.first);            // 10
    var q: I64Pair = I64Pair{ .first = 100, .second = 200 };  // distinct instantiation
    print(q.first);            // 100
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "42\n10\n100\n");
}

// --- v0.130 generic-struct methods + ArrayList(T) --------------------------

#[test]
fn generic_struct_methods() {
    let src = r#"
fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn get(self: Self) T { return self.v; }
        fn doubled(self: Self) T { return self.v + self.v; }
    };
}
const IB = Box(i32);
pub fn main() i32 {
    var b: IB = IB{ .v = 21 };
    print(b.get());        // 21
    print(b.doubled());    // 42
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "21\n42\n");
}

#[test]
fn generic_arraylist_grows_on_the_allocator() {
    let src = r#"
fn ArrayList(comptime T: type) type {
    return struct {
        items: []T,
        count: usize,
        fn init(a: Allocator) Self {
            return Self{ .items = alloc(a, T, 4), .count = 0 };
        }
        fn append(self: Self, a: Allocator, x: T) Self {
            if (self.count < self.items.len) {
                var here: []T = self.items;
                here[self.count] = x;
                return Self{ .items = here, .count = self.count + 1 };
            }
            var nb: []T = alloc(a, T, self.items.len * 2);
            var i: usize = 0;
            while (i < self.count) : (i = i + 1) { nb[i] = self.items[i]; }
            free(a, self.items);
            nb[self.count] = x;
            return Self{ .items = nb, .count = self.count + 1 };
        }
        fn get(self: Self, i: usize) T { return self.items[i]; }
        fn len(self: Self) usize { return self.count; }
        fn deinit(self: Self, a: Allocator) void { free(a, self.items); }
    };
}
const IntList = ArrayList(i32);
pub fn main() i32 {
    var a: Allocator = c_allocator();
    var list: IntList = IntList.init(a);     // grows past the initial cap of 4
    var i: i32 = 0;
    while (i < 10) : (i = i + 1) { list = list.append(a, i * i); }
    print(list.len());        // 10
    print(list.get(9));       // 81
    var total: i32 = 0;
    var j: usize = 0;
    while (j < list.len()) : (j = j + 1) { total = total + list.get(j); }
    print(total);             // 285
    list.deinit(a);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "10\n81\n285\n");
}

// --- v0.131 compound assignment --------------------------------------------

#[test]
fn compound_assignment_operators() {
    let src = r#"
pub fn main() i32 {
    var x: i32 = 10;
    x += 5;   print(x);     // 15
    x -= 3;   print(x);     // 12
    x *= 4;   print(x);     // 48
    x /= 5;   print(x);     // 9
    x %= 4;   print(x);     // 1
    var a: [3]i32 = [3]i32{ 10, 20, 30 };
    var i: usize = 1;
    a[i] += 100;            // a[1] = 120
    print(a[1]);            // 120
    var sum: i32 = 0;
    var k: i32 = 0;
    while (k < 5) : (k += 1) { sum += k; }  // compound in the continue-clause + body
    print(sum);             // 10
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "15\n12\n48\n9\n1\n120\n10\n");
}

// --- v0.132 bitwise & shift operators --------------------------------------

#[test]
fn bitwise_and_shift_operators() {
    let src = r#"
const MASK: i32 = (1 << 8) - 1;   // 255, folded at compile time
pub fn main() i32 {
    var a: i32 = 12;
    var b: i32 = 10;
    print(a & b);        // 8
    print(a | b);        // 14
    print(a ^ b);        // 6
    print(a << 2);       // 48
    print(a >> 1);       // 6
    print(~a);           // -13
    print(MASK);         // 255
    print(a | b & 4);    // 12  (& binds tighter than |, and b&4 == 0)
    var x: i32 = 99;
    var p: *i32 = &x;    // prefix & (address-of) is unaffected by infix bitand
    print(p.*);          // 99
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "8\n14\n6\n48\n6\n-13\n255\n12\n99\n");
}

// --- v0.133 for-loops over arrays & slices ---------------------------------

#[test]
fn for_loops_over_arrays_and_slices() {
    let src = r#"
fn sum(xs: []i32) i32 {
    var total: i32 = 0;
    for (xs) |x| { total += x; }
    return total;
}
pub fn main() i32 {
    var a: [5]i32 = [5]i32{ 10, 20, 30, 40, 50 };
    print(sum(a[0..5]));          // 150 (for over a slice)
    var found: usize = 99;
    for (a, 0..) |x, i| {          // index form
        if (x > 25) {
            if (found == 99) { found = i; }
        }
    }
    print(found);                 // 2
    var evens: i32 = 0;
    for (a) |x| {
        if (x % 20 != 0) { continue; }   // continue must still advance the index
        evens += 1;
    }
    print(evens);                 // 2
    var product: i32 = 1;
    var small: [3]i32 = [3]i32{ 2, 3, 4 };
    for (small) |x| { product *= x; }    // for over an array directly
    print(product);               // 24
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "150\n2\n2\n24\n");
}

// --- v0.134 pointer-receiver methods (true mutation) -----------------------

#[test]
fn pointer_receiver_methods_mutate() {
    let src = r#"
const Counter = struct {
    n: i32,
    fn inc(self: *Counter) void { self.n += 1; }        // pointer receiver mutates
    fn add(self: *Counter, by: i32) void { self.n += by; }
    fn get(self: Counter) i32 { return self.n; }         // value receiver unchanged
};
fn reset(c: *Counter) void { c.n = 0; }                  // write through a *Struct param
pub fn main() i32 {
    var c: Counter = Counter{ .n = 0 };
    c.inc();
    c.inc();
    print(c.get());        // 2 (real mutation, auto-ref &c)
    c.add(40);
    print(c.get());        // 42
    reset(&c);
    print(c.get());        // 0
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "2\n42\n0\n");
}

#[test]
fn pointer_receiver_generic_arraylist_push() {
    // Pointer receivers on a generic struct: a mutating `push` (no value-return).
    let src = r#"
fn ArrayList(comptime T: type) type {
    return struct {
        items: []T,
        count: usize,
        fn init(a: Allocator) Self { return Self{ .items = alloc(a, T, 4), .count = 0 }; }
        fn push(self: *Self, a: Allocator, x: T) void {
            if (self.count == self.items.len) {
                var nb: []T = alloc(a, T, self.items.len * 2);
                var i: usize = 0;
                while (i < self.count) : (i += 1) { nb[i] = self.items[i]; }
                free(a, self.items);
                self.items = nb;     // write a slice field through the pointer
            }
            self.items[self.count] = x;
            self.count += 1;          // compound through the pointer
        }
        fn get(self: Self, i: usize) T { return self.items[i]; }
        fn len(self: Self) usize { return self.count; }
        fn deinit(self: Self, a: Allocator) void { free(a, self.items); }
    };
}
const IntList = ArrayList(i32);
pub fn main() i32 {
    var a: Allocator = c_allocator();
    var list: IntList = IntList.init(a);
    var i: i32 = 0;
    while (i < 6) : (i += 1) { list.push(a, i * i); }   // mutates in place, no reassign
    print(list.len());        // 6
    print(list.get(5));       // 25
    list.deinit(a);
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "6\n25\n");
}

// --- v0.135 multiple type parameters ---------------------------------------

#[test]
fn multiple_type_parameters() {
    let src = r#"
fn Pair(comptime A: type, comptime B: type) type {
    return struct {
        first: A,
        second: B,
        fn set(self: *Self, a: A, b: B) void {   // a method using BOTH type params
            self.first = a;
            self.second = b;
        }
        fn fst(self: Self) A { return self.first; }
        fn snd(self: Self) B { return self.second; }
    };
}
const IL = Pair(i32, i64);
const LB = Pair(i64, i32);   // distinct instantiation (argument order)
pub fn main() i32 {
    var p: IL = IL{ .first = 5, .second = 9 };
    print(p.fst());          // 5
    print(p.snd());          // 9
    p.set(7, 11);            // pointer-receiver mutation
    print(p.fst());          // 7
    print(p.snd());          // 11
    var q: LB = LB{ .first = 100, .second = 3 };
    print(q.snd());          // 3
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "5\n9\n7\n11\n3\n");
}

// --- v0.136 comptime reflection builtins (@sizeOf/@typeName/@This) ----------

#[test]
fn comptime_reflection_builtins() {
    let src = r#"
const Point = struct {
    x: i32,
    y: i32,
    fn at_origin(self: *@This()) bool {     // @This() in a plain struct
        return self.x == 0 and self.y == 0;
    }
    fn shift(self: *Self, d: i32) void {    // Self also works in a plain struct
        self.x += d;
        self.y += d;
    }
};
pub fn main() i32 {
    print(@sizeOf(i32));        // 4
    print(@sizeOf(i64));        // 8
    print(@sizeOf(Point));      // 8
    var p: Point = Point{ .x = 0, .y = 0 };
    if (p.at_origin()) { print(1); } else { print(0); }   // 1
    p.shift(5);
    print(p.x);                 // 5
    if (p.at_origin()) { print(1); } else { print(0); }   // 0
    print(@typeName(i32));      // i32
    print(@typeName(Point));    // Point
    return 0;
}
"#;
    let (code, out) = build_and_capture(src, EmitMode::Program);
    assert_eq!(code, 0);
    assert_eq!(out, "4\n8\n8\n1\n5\n0\ni32\nPoint\n");
}
