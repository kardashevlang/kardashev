//SPEC: §9.1 a struct literal initialises every declared field by name, exactly once
//OUT: 6
//OUT: 35
//OUT: 41
// Each field is derived from the seed through a different computation, so a
// literal that bound values to the wrong fields would change every line below.
const Triple = struct {
    a: i32,
    b: i32,
    c: i32,
};

fn make(seed: i32) Triple {
    return Triple{ .a = seed + 1, .b = seed * seed, .c = seed * 8 };
}

pub fn main() void {
    var t: Triple = make(5);   // a = 6, b = 25, c = 40
    print(t.a);                // 6
    print(t.b + 10);           // 35
    print(t.c + 1);            // 41
}
