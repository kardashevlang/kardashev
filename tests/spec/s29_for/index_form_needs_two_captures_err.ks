//SPEC: §29 the `, 0..` index form requires exactly TWO captures `|elem, index|` — one capture is rejected
//ERR: E0200

pub fn main() void {
    var xs: [2]i64 = [2]i64{ 1, 2 };
    for (xs, 0..) |v| {
        print(v);
    }
}
