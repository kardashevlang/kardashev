//SPEC: §14.1 array literal elements are full expressions, each coerced to the element type
//OUT: 9
//OUT: 15
//OUT: 99

fn sq(x: i64) i64 {
    return x * x;
}

pub fn main() void {
    var b: i64 = 10;
    // Calls, locals and arithmetic all compute element values at runtime.
    var a: [3]i64 = [3]i64{ sq(3), b + 5, sq(b) - 1 };
    print(a[0]);
    print(a[1]);
    print(a[2]);
}
