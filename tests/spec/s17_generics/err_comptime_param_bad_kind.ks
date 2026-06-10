//SPEC: §17.2 a comptime parameter that is neither `type` nor an integer value kind is E0250
//ERR: E0250

// `bool` is not `type` and not an integer type, so it can be neither a type
// parameter (§17) nor a comptime value parameter (§24). The declaration alone
// is diagnosed — no call required.
fn pick(comptime flag: bool, a: i64, b: i64) i64 {
    if (flag) {
        return a;
    }
    return b;
}

pub fn main() void {
    print(0);
}
