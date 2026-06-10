//SPEC: §24.1 a comptime value parameter must be a BARE integer type — a composite wrapper (here a slice of an integer type) is E0250
//ERR: E0250

// `[]u8` has an integer element but the slice wrapper disqualifies it: a
// comptime parameter is either `type` (§17) or a bare integer value (§24).
// (The plain non-integer case, `comptime flag: bool`, is pinned in s17.)
fn bad(comptime s: []u8, x: i64) i64 {
    return x;
}

pub fn main() void {
    print(bad("hi", 1));
}
