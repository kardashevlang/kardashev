// panic.ks — runtime-safety primitives `@panic` and `unreachable` (v0.141).
//
//   @panic(msg)    write `msg` (a []u8) to stderr and exit with code 101.
//   unreachable    trap (exit 101) if control ever reaches it.
//
// Both diverge — they never return — so they may stand in any value position
// (e.g. the `else` arm of a switch the programmer knows is total).

// A total classifier: callers only pass 0, 1, or 2, so any other value is a bug.
fn name_of(kind: i32) []u8 {
    switch (kind) {
        0 => { return "red"; },
        1 => { return "green"; },
        2 => { return "blue"; },
        else => { unreachable; },
    }
}

// Bounds-checked division: division by zero is a programming error here.
fn div(a: i32, b: i32) i32 {
    if (b == 0) {
        @panic("div: division by zero");
    }
    return a / b;
}

pub fn main() i32 {
    print(name_of(0));      // red
    print(name_of(2));      // blue
    print(div(84, 2));      // 42
    print(div(100, 5));     // 20

    // The next call traps: it prints "div: division by zero" to stderr and the
    // process exits 101. Nothing after it runs.
    print(div(1, 0));
    print(0 - 1);           // never reached
    return 0;
}

test "safe paths do not trap" {
    expect(div(20, 4) == 5);
    expect(div(0 - 9, 3) == 0 - 3);
    var n: []u8 = name_of(1);
    expect(n.len == 5);     // "green"
}
