// QUARANTINED (wave B, v0.156): compiler/SPEC contradiction — `!void`.
//
// sema accepts an error union over `void` (`fn f() !void`, `try`, `catch`)
// with no diagnostic, but emit_c's §12.3 recipe then produces invalid C:
//
//   typedef struct { int32_t err; void val; } kd_err_void;   // `void val` !!
//   static inline void kd_err_void_catch(kd_err_void e, void d) { ... }
//
// so the cc step fails on every program that mentions `!void`. Either sema
// should reject a `void` payload (like the inexpressible `![]T`, §41 note) or
// the backend needs a payload-less error-union representation. Until decided,
// this repro is quarantined; nothing in tests/spec pins `!void` behaviour.
//
// Intended directives if the lazy capturing form worked over `!void`:
//SPEC: §36.1 (quarantined) `catch |e|` over a `!void` callee runs the handler on the error path only
// (no //OUT//EXIT — the file is NOT in the runner's walk; tests/spec-quarantine
// is outside tests/spec.)

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
    g(9) catch |e| print(e);
}
