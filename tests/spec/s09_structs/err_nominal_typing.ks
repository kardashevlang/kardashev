//SPEC: §9.3 struct types are nominal — same-shaped structs are distinct types (equal iff same id), so mixing them is E0110
//ERR: E0110
const A = struct {
    x: i32,
};
const B = struct {
    x: i32,
};

fn take_a(a: A) i32 {
    return a.x;
}

pub fn main() void {
    var b: B = B{ .x = 1 };
    var a: A = b;        // identical shape, different struct id
    print(take_a(b));    // same mismatch through a parameter
}
