//SPEC: §9.1 field-init order in a struct literal is free — inits bind by name, not position
//OUT: 39
//OUT: 39
//OUT: 6
// `fwd` writes fields in declaration order, `rev` in reverse order. If literal
// inits bound positionally, `rev` would swap first/second and the encoded
// digits below would differ between the two.
const Pair = struct {
    first: i32,
    second: i32,
};

fn fwd(x: i32, y: i32) Pair {
    return Pair{ .first = x, .second = y };
}

fn rev(x: i32, y: i32) Pair {
    return Pair{ .second = y, .first = x };
}

pub fn main() void {
    var a: Pair = fwd(3, 9);
    var b: Pair = rev(3, 9);
    print(a.first * 10 + a.second);   // 39
    print(b.first * 10 + b.second);   // 39 — identical despite reversed inits
    print(b.second - b.first);        // 6
}
