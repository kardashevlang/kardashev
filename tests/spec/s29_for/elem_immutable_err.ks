//SPEC: §29.1 `elem` is an immutable binding — assigning to it is rejected
//ERR: E0110

pub fn main() void {
    var xs: [2]i64 = [2]i64{ 1, 2 };
    for (xs) |v| {
        v = 0;
    }
}
