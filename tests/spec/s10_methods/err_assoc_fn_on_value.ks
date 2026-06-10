//SPEC: §10 calling an associated function on a value receiver is E0172
//ERR: E0172
const C = struct {
    n: i32,

    fn zero() C {
        return C{ .n = 0 };
    }
};

pub fn main() void {
    var c: C = C{ .n = 1 };
    var d: C = c.zero();   // `zero` has no self; it must be called as C.zero()
    print(d.n);
}
