//SPEC: §12.3 `!void` lowers to a payload-less `{ err }` union; `try f();` propagates, `catch` runs its (void) handler on the error path only
//OUT: 1
//OUT: 100
//OUT: 1

// Was quarantined (wave B, v0.156): sema accepted `!void` but emit produced
// `typedef struct { int32_t err; void val; }` — invalid C. Fixed: a void
// payload drops the `val` field; `try`/`catch`/returns special-case it.
// NOTE the success path of `g(1) catch print(0 - 1)` pins that the handler
// did NOT run (stdout must equal exactly the three lines above) — a `catch`
// over `!void` is necessarily lazy (there is no payload to select eagerly).

fn f(n: i64) !void {
    if (n > 2) {
        return error.Big;
    }
    print(n);
}

fn g(n: i64) !void {
    try f(n);
    print(100);
}

pub fn main() void {
    g(1) catch print(0 - 1);
    g(9) catch |e| print(e); // error.Big is code 1
}
