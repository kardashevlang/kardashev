//SPEC: §36.1 `expr catch |e| default` binds the error code (an `i32`) inside the handler; ok values skip it
//OUT: 12
//OUT: 1
//OUT: 107

// This program mentions exactly one error name, so its 1-based code (§12.1)
// must be 1 — making the captured value deterministic.
fn f(n: i32) !i32 {
    if (n == 0) {
        return error.Only;
    }
    return n * 3;
}

pub fn main() void {
    print(f(4) catch |e| e);             // ok: handler skipped, payload 12
    print(f(0) catch |e| e);             // error: e == 1 (the sole code)
    print(f(0) catch |e| e * 100 + 7);   // the handler is a full expression: 107
}
