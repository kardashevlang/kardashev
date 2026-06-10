//SPEC: §10 a method call's receiver may be any struct VALUE — including a slice element `s[i].m()`
//OUT: 11

// QUARANTINED (wave B, v0.156): the compiler contradicts SPEC §10 here.
//
// sema accepts `s[i].get()` where `s: []C` (the receiver IS a struct value),
// but emit_c resolves the receiver struct's name to "" and generates a call
// to `kd__get` instead of `kd_C_get`, so the C compile fails:
//
//   error: implicit declaration of function 'kd__get'; did you mean 'kd_C_get'?
//
// The bug is independent of v0.152 direct generics — it reproduces with a
// plain named struct (below) and with alias/application-typed slices alike.
// ARRAY elements work (`arr[i].get()`); only SLICE-indexed receivers break.
// Workaround used by the live corpus: copy the element to a local first
// (`var e: C = s[i]; e.get()`) or read fields directly (`s[i].n`).
//
// Move this file into tests/spec/s09_structs/ (or s15_ptr_slices/) once
// emit_c's method-receiver type resolution handles Index-into-slice.

const C = struct {
    n: i64,
    fn get(self: C) i64 {
        return self.n;
    }
};

fn total(s: []C) i64 {
    var t: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        t = t + s[i].get();   // <- miscompiles to kd__get(...)
    }
    return t;
}

pub fn main() void {
    var arr: [2]C = [2]C{ C{ .n = 5 }, C{ .n = 6 } };
    print(total(arr[0..2]));
}
