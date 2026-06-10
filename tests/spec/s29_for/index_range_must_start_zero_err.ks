//SPEC: §29 the index form is literally `, 0..` — a range starting anywhere else is rejected
//ERR: E0200

pub fn main() void {
    var xs: [2]i64 = [2]i64{ 1, 2 };
    for (xs, 1..) |v, i| {
        print(v);
    }
}
