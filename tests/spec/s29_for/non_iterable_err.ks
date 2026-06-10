//SPEC: §29.1 the iterable must be a `[N]T` array or `[]T` slice — an integer is rejected
//ERR: E0300

pub fn main() void {
    for (42) |v| {
        print(v);
    }
}
