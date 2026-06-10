//SPEC: §10 a method call's receiver may be any struct VALUE — including a slice element `s[i].m()`
//OUT: 11

// Was quarantined (wave B, v0.156): emit_c's receiver resolution
// (`struct_of_expr`) handled ARRAY-indexed receivers but had no slice arm, so
// `s[i].get()` resolved to an empty struct name (`kd__get`, implicit-
// declaration cc failure). Fixed: an `Index` whose base is a slice resolves
// the element struct exactly like an array element.

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
        t = t + s[i].get();
    }
    return t;
}

pub fn main() void {
    var arr: [2]C = [2]C{ C{ .n = 5 }, C{ .n = 6 } };
    print(total(arr[0..2]));
}
